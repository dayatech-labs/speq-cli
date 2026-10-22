# Dayaboard workflow API: what this skill uses

A summary of https://dayaboard.odyssey.co.id/llms-full.txt (October 2026). The live document is the source of truth: read it when something here is missing or Dayaboard disagrees. Every call below runs as `speq dayaboard <METHOD> <path> [--data …]`, which adds the API key; `/api/workflow/...` paths in Dayaboard's document are aliases of these `/api/v1/workflow/...` paths.

## Projects and sprints

- `GET /api/v1/workflow/projects`: the active projects the user can access, each with `id`, `key`, `name`, `canManage`.
- `GET /api/v1/workflow/sprints?projectKey=<KEY>` (or `projectId=<uuid>`), `&activeOnly=true` for the active sprint.

## Reading issues

- `GET /api/v1/workflow/issues?projectKey=<KEY>&limit=<n>`: list. Keyword search with `q=<text>`.
- `GET /api/v1/workflow/issues/<key or uuid>`: one issue (core fields, status, description, comments, attachments). `?fields=a,b` picks fields (`id` and `issueKey` are always returned).

## Creating an issue: `POST /api/v1/workflow/issues`

Send an `Idempotency-Key` (UUID) with every create: the same key and body again return the saved answer (`Idempotency-Replayed: true`) instead of a second ticket. Keys last 24 hours, per account.

| Field | Required | Values |
| --- | --- | --- |
| `projectKey` (or `projectId`) | yes | project key such as `DEV`, or its name; `projectId` is a UUID |
| `summary` | yes | at least 1 character |
| `issueType` (or `issueTypeId`) | yes | `Epic`, `Story`, `Task`, `Bug`, `Spike` |
| `storyPoints`, `manDays`, `manHours` | at least one, > 0 | numbers; 1 man-day = 8 man-hours |
| `description` | no | markdown; `@email` mentions a user |
| `assignee` (or `assigneeId`) | no | email, display name, or user UUID |
| `priority` (or `priorityId`) | no | `Highest`, `High`, `Medium` (default), `Low` |
| `status` (or `statusId`) | no | a status name such as `To Do`, `In Progress`, `Done`; default: the project's first status |
| `team` | no | up to 50 characters, e.g. `BE`, `FE`, `QA`; default: the assignee's team |
| `sprint` (or `sprintId`, `sprintName`) | no | `active`, a sprint name, or a UUID; empty or `backlog` = backlog |
| `startDate`, `dueDate` | no | `YYYY-MM-DD` |
| `parentId` | no | parent issue key or UUID, in the same project |
| `linkedIssues` | no | `[{"targetIssueId": "<key>", "linkType": "<type>"}]` |

The answer is the created issue, including its `issueKey`.

Example body:

```json
{
  "projectKey": "DEV",
  "issueType": "Task",
  "summary": "[BE][CMS][Payment] - Create the card charge endpoint",
  "description": "...\n\n> Speq: mcat st-003 be-001 · Path: [epics/mcat/stories/st-003-pay-by-card/tasks/be-001-create-charge.md](https://speq.dayatech.workers.dev/p/<project_id>/epics/mcat/stories/st-003-pay-by-card/tasks/be-001-create-charge.md) · Synced: 3f2a9c1d8e7b",
  "parentId": "DEV-41",
  "team": "BE",
  "manDays": 2
}
```

## Updating: `PATCH /api/v1/workflow/issues/<id>`

Dayaboard documents this path with the issue `id` (UUID), which `GET /api/v1/workflow/issues/<key>` returns.

Only the fields sent change: `status`, `assignee` (`null` or `"unassigned"` clears it), `summary`, `description`, `priority`, `issueType`, `team`, `storyPoints`, `manDays`, `manHours`, `startDate`, `dueDate`, `sprint` (`null`, `backlog`, or `none` moves the issue to the backlog). `parentId`, `linkedIssues`, and the project cannot be changed here.

## Links and comments

- `POST /api/v1/workflow/issues/<key>/links` with `{"targetIssueId": "<key>", "linkType": "relates to" | "blocks" | "is blocked by" | "duplicates"}`.
- `POST /api/v1/workflow/issues/<key>/comments` with `{"content": "<text>"}`; `PUT .../comments/<commentId>` edits only the user's own comment.

## Limits

- 120 requests per minute per IP; a `429` names `Retry-After`.
- API keys cannot `DELETE` anything (`403 forbidden_operation`) or manage API keys. Those are done in the Web app.
- Errors are `{ "error": "<code>", "message": "…", "details": … }`. The codes and what to do are in `SKILL.md` §6.
