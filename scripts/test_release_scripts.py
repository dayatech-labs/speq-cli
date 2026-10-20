#!/usr/bin/env python3
"""Tests for next_version.py and stamp_version.py. Run: python3 scripts/test_release_scripts.py"""
import functools
import contextlib
import hashlib
import http.server
import os
import pathlib
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import next_version  # noqa: E402


def git(repo, *args):
    subprocess.run(["git", "-C", repo, "-c", "user.name=t", "-c", "user.email=t@example.test", *args], check=True, capture_output=True)


def repo_with(*steps):
    """steps: ('commit', message) or ('tag', name)."""
    d = tempfile.mkdtemp()
    git(d, "init", "-q", "-b", "main")
    for kind, value in steps:
        if kind == "commit":
            git(d, "commit", "-q", "--allow-empty", "-m", value)
        else:
            git(d, "tag", value)
    return d


def decide(*steps):
    version, _ = next_version.plan(repo_with(*steps))
    return None if version is None else ".".join(map(str, version))


class NextVersion(unittest.TestCase):
    def test_first_release_needs_a_releasable_commit(self):
        self.assertEqual(decide(("commit", "feat: first")), "0.1.0")
        self.assertEqual(decide(("commit", "fix: first")), "0.1.0")
        self.assertIsNone(decide(("commit", "docs: readme"), ("commit", "chore: tidy")))

    def test_bumps_follow_the_commit_types(self):
        base = [("commit", "feat: x"), ("tag", "v1.2.3")]
        self.assertEqual(decide(*base, ("commit", "fix: bug")), "1.2.4")
        self.assertEqual(decide(*base, ("commit", "feat(api): more")), "1.3.0")
        self.assertEqual(decide(*base, ("commit", "fix: a"), ("commit", "feat: b")), "1.3.0")
        self.assertEqual(decide(*base, ("commit", "feat!: drop it")), "2.0.0")
        self.assertEqual(decide(*base, ("commit", "fix: a\n\nBREAKING CHANGE: gone")), "2.0.0")
        self.assertEqual(decide(*base, ("commit", "refactor: tidy")), "1.2.4")

    def test_no_release_for_non_code_commits_or_nothing_new(self):
        base = [("commit", "feat: x"), ("tag", "v1.2.3")]
        self.assertIsNone(decide(*base))
        self.assertIsNone(decide(*base, ("commit", "docs: d"), ("commit", "ci: c"), ("commit", "test: t"), ("commit", "chore: k")))
        self.assertIsNone(decide(*base, ("commit", "update stuff")))

    def test_breaking_before_1_0_bumps_minor_and_latest_tag_wins(self):
        self.assertEqual(decide(("commit", "feat: x"), ("tag", "v0.4.0"), ("commit", "feat!: y")), "0.5.0")
        self.assertEqual(decide(("commit", "feat: x"), ("tag", "v0.9.0"), ("tag", "v0.10.0"), ("commit", "fix: z")), "0.10.1")


class Stamp(unittest.TestCase):
    def test_stamps_cargo_toml_and_lock_together(self):
        d = pathlib.Path(tempfile.mkdtemp())
        (d / "Cargo.toml").write_text('[package]\nname = "speq"\nversion = "0.0.0"\n\n[dependencies]\nclap = { version = "4.6.7" }\n')
        (d / "Cargo.lock").write_text('[[package]]\nname = "clap"\nversion = "4.6.7"\n\n[[package]]\nname = "speq"\nversion = "0.0.0"\ndependencies = [\n "clap",\n]\n')
        subprocess.run([sys.executable, str(HERE / "stamp_version.py"), "2.5.1"], check=True, cwd=d)
        self.assertIn('version = "2.5.1"', (d / "Cargo.toml").read_text())
        self.assertIn('version = "4.6.7"', (d / "Cargo.toml").read_text())
        lock = (d / "Cargo.lock").read_text()
        self.assertIn('name = "speq"\nversion = "2.5.1"', lock)
        self.assertIn('name = "clap"\nversion = "4.6.7"', lock)
        bad = subprocess.run([sys.executable, str(HERE / "stamp_version.py"), "1.2"], cwd=d)
        self.assertNotEqual(bad.returncode, 0)


def rust_banner():
    """The logo rows exactly as `speq help` prints them (BANNER in src/cli.rs)."""
    src = (HERE.parent / "src" / "cli.rs").read_text()
    block = src[src.index("const BANNER") :]
    block = block[: block.index("];")]
    return re.findall(r'r"(.*)",', block)


def installer_target():
    system, machine = platform.system(), platform.machine()
    if system == "Darwin":
        return {"arm64": "aarch64-apple-darwin", "x86_64": "x86_64-apple-darwin"}.get(machine)
    if system == "Linux" and machine in ("x86_64", "amd64"):
        return "x86_64-unknown-linux-musl"
    return None


@contextlib.contextmanager
def serve(directory):
    """Serves a local release mirror; yields its base URL."""

    class Quiet(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *args):
            pass

    handler = functools.partial(Quiet, directory=str(directory))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()


class Installer(unittest.TestCase):
    def test_banner_matches_the_cli_in_both_installers(self):
        rows = rust_banner()
        self.assertEqual(len(rows), 5)
        for name in ("install.sh", "install.ps1"):
            text = (HERE / name).read_text()
            self.assertIn("\n".join(rows), text, f"{name} banner differs from src/cli.rs")

    def run_installer(self, tmp, *args, fake_git=None):
        """Runs install.sh against a local mirror, detached from any terminal (as in CI)."""
        target = installer_target()
        release = tmp / "mirror" / "download" / "v0.1.0"
        release.mkdir(parents=True)
        name = f"speq-0.1.0-{target}"
        binary = release / name
        binary.write_text("#!/bin/sh\necho speq 0.1.0\n")
        digest = hashlib.sha256(binary.read_bytes()).hexdigest()
        (release / "SHA256SUMS").write_text(f"{digest}  {name}\n")

        with serve(tmp / "mirror") as url:
            home = tmp / "home"
            home.mkdir()
            env = {**os.environ, "HOME": str(home), "SPEQ_RELEASE_URL": url}
            env.pop("NO_COLOR", None)
            if fake_git is not None:
                fake = tmp / "fakebin"
                fake.mkdir()
                (fake / "git").write_text(f"#!/bin/sh\n{fake_git}\n")
                (fake / "git").chmod(0o755)
                env["PATH"] = f"{fake}:{env['PATH']}"
            done = subprocess.run(
                ["sh", str(HERE / "install.sh"), "--version", "0.1.0", *args],
                env=env,
                capture_output=True,
                text=True,
                check=True,
                stdin=subprocess.DEVNULL,
                start_new_session=True,
                timeout=60,
            )
        return home, done.stdout

    @unittest.skipUnless(installer_target(), "install.sh supports macOS and x86_64 Linux only")
    def test_success_shows_the_banner_after_the_install_line(self):
        with tempfile.TemporaryDirectory() as tmp:
            home, out = self.run_installer(pathlib.Path(tmp), fake_git="echo 'git version 2.54.0'")
            bin_dir = home / ".local" / "bin"
            self.assertTrue((bin_dir / "speq").exists())
            self.assertIn("\n".join(rust_banner()), out)
            self.assertLess(out.index("Installed speq 0.1.0"), out.index(rust_banner()[0]))
            self.assertNotIn("\x1b", out, "no colors when stdout is not a terminal")
            self.assertNotIn("Git 2.28 or newer was not found", out)

    @unittest.skipUnless(installer_target(), "install.sh supports macOS and x86_64 Linux only")
    def test_git_helpers_are_links_to_the_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            home, _ = self.run_installer(pathlib.Path(tmp), fake_git="echo 'git version 2.54.0'")
            bin_dir = home / ".local" / "bin"
            for helper in ("git-remote-speq", "git-credential-speq"):
                self.assertTrue((bin_dir / helper).is_symlink(), helper)
                self.assertEqual(os.readlink(bin_dir / helper), "speq")

    @unittest.skipUnless(installer_target(), "install.sh supports macOS and x86_64 Linux only")
    def test_a_missing_git_is_offered_but_never_blocks_without_a_terminal(self):
        for fake in ("echo 'git version 2.20.1'", "echo 'xcrun: error' >&2; exit 1"):
            with tempfile.TemporaryDirectory() as tmp:
                home, out = self.run_installer(pathlib.Path(tmp), fake_git=fake)
                self.assertTrue((home / ".local" / "bin" / "speq").exists())
                self.assertIn("Git 2.28 or newer was not found", out)
                self.assertTrue("Skipped. Install it later with:" in out or "Install Git with your package manager" in out, out)

    @unittest.skipUnless(installer_target(), "install.sh supports macOS and x86_64 Linux only")
    def test_no_git_skips_the_check(self):
        with tempfile.TemporaryDirectory() as tmp:
            _, out = self.run_installer(pathlib.Path(tmp), "--no-git", fake_git="exit 1")
            self.assertNotIn("Git 2.28", out)


# Plain http is for a loopback test mirror only; these merely start like one.
LOOKALIKE_URLS = (
    "http://127.0.0.1.attacker.test",
    "http://localhost.attacker.test/dl",
    "http://localhost@attacker.test",
    "http://example.test",
)


class ReleaseUrl(unittest.TestCase):
    def run_sh(self, url):
        return subprocess.run(
            ["sh", str(HERE / "install.sh"), "--version", "0.1.0"],
            env={**os.environ, "SPEQ_RELEASE_URL": url},
            capture_output=True,
            text=True,
            stdin=subprocess.DEVNULL,
            timeout=60,
        )

    def test_install_sh_allows_plain_http_only_on_loopback(self):
        for url in LOOKALIKE_URLS:
            with self.subTest(url=url):
                done = self.run_sh(url)
                self.assertNotEqual(done.returncode, 0)
                self.assertIn("the release URL must use https", done.stderr)
        # Nothing listens on port 1: these pass the URL check and fail later, on the download or the platform.
        for url in ("http://127.0.0.1:1", "http://localhost:1/mirror"):
            with self.subTest(url=url):
                done = self.run_sh(url)
                self.assertNotEqual(done.returncode, 0)
                self.assertNotIn("must use https", done.stderr)

    def test_install_sh_downloads_and_follows_redirects_over_https_only(self):
        text = (HERE / "install.sh").read_text()
        fetch = re.search(r"^fetch\(\) \{ curl (.*)\}$", text, re.M).group(1)
        self.assertIn('--proto "$protocols"', fetch)
        self.assertIn('--proto-redir "$protocols"', fetch)
        self.assertIn("https://*) protocols='=https' ;;", text)


@unittest.skipUnless(shutil.which("pwsh"), "pwsh is not installed")
class WindowsInstaller(unittest.TestCase):
    """install.ps1 reads the OS variables, which Windows PowerShell 5.1 and PowerShell 7 both set."""

    NO_BUILD = "this release has no build for x86_64-pc-windows-msvc"

    def run_ps1(self, **arch_vars):
        """Runs install.ps1 against a mirror with no Windows build, so it stops right after the architecture check."""
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            release = tmp / "mirror" / "download" / "v0.1.0"
            release.mkdir(parents=True)
            (release / "SHA256SUMS").write_text(f"{'0' * 64}  speq-0.1.0-x86_64-apple-darwin\n")
            env = {k: v for k, v in os.environ.items() if k not in ("PROCESSOR_ARCHITECTURE", "PROCESSOR_ARCHITEW6432")}
            env.update(arch_vars)
            env["LOCALAPPDATA"] = str(tmp / "appdata")
            with serve(tmp / "mirror") as url:
                env["SPEQ_RELEASE_URL"] = url
                done = subprocess.run(
                    ["pwsh", "-NoProfile", "-NonInteractive", "-File", str(HERE / "install.ps1"), "-Version", "0.1.0"],
                    env=env,
                    capture_output=True,
                    text=True,
                    stdin=subprocess.DEVNULL,
                    timeout=120,
                )
        self.assertNotEqual(done.returncode, 0)
        return done.stdout + done.stderr

    def test_x64_passes_the_architecture_check(self):
        out = self.run_ps1(PROCESSOR_ARCHITECTURE="AMD64")
        self.assertIn("Fetching the release manifest", out)
        self.assertIn(self.NO_BUILD, out)
        self.assertNotIn("unsupported architecture", out)

    def test_32_bit_powershell_on_x64_windows_passes(self):
        out = self.run_ps1(PROCESSOR_ARCHITECTURE="x86", PROCESSOR_ARCHITEW6432="AMD64")
        self.assertIn(self.NO_BUILD, out)
        self.assertNotIn("unsupported architecture", out)

    def test_other_architectures_are_named_and_stop_before_any_download(self):
        for arch_vars, shown in (({"PROCESSOR_ARCHITECTURE": "ARM64"}, "'ARM64'"), ({"PROCESSOR_ARCHITECTURE": "x86"}, "'x86'"), ({}, "''")):
            with self.subTest(arch_vars=arch_vars):
                out = self.run_ps1(**arch_vars)
                self.assertIn(f"unsupported architecture: {shown}", out)
                self.assertNotIn("Fetching the release manifest", out)

    def test_plain_http_is_allowed_only_on_loopback(self):
        for url in LOOKALIKE_URLS:
            with self.subTest(url=url):
                done = subprocess.run(
                    ["pwsh", "-NoProfile", "-NonInteractive", "-File", str(HERE / "install.ps1")],
                    env={**os.environ, "SPEQ_RELEASE_URL": url},
                    capture_output=True,
                    text=True,
                    stdin=subprocess.DEVNULL,
                    timeout=120,
                )
                self.assertNotEqual(done.returncode, 0)
                self.assertIn("the release URL must use https", done.stdout + done.stderr)


if __name__ == "__main__":
    unittest.main()
