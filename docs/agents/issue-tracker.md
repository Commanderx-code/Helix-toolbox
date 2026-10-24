# Issue tracker: GitHub

Issues and specs for this repository are GitHub issues. Use the `gh` command
line for every operation; run inside a clone, it finds the repository from
`git remote -v`.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`, with a
  heredoc for a body of several lines.
- **Read an issue**:
  `gh issue view <number> --json number,title,body,labels,comments`.
- **List issues**, with `--label` and `--state` filters as needed:

  ```sh
  gh issue list --state open --json number,title,body,labels,comments \
    --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'
  ```

- **Make an issue a sub-issue of a parent**: `gh issue create --parent
  <parent> ...`, or `gh issue edit <parent> --add-sub-issue <child>`
  afterward (`gh` 2.94 or later). With an older `gh`:

  ```sh
  gh api --method POST repos/<owner>/<repo>/issues/<parent>/sub_issues -F sub_issue_id=<child-database-id>
  ```

  The database id is the one **Blocking** below describes.
- **Comment**: `gh issue comment <number> --body "..."`.
- **Add or remove labels**: `gh issue edit <number> --add-label "..."` or
  `--remove-label "..."`.
- **Close**: `gh issue close <number> --comment "..."`.

## Pull requests as a triage surface

**PRs as a request surface: no.** Pull requests are not triaged as feature
requests; `/triage` reads this flag.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Read it as in **Read an issue** above. GitHub numbers issues and pull
requests together, so a bare `#42` may be either: try `gh pr view 42`, then
`gh issue view 42`.

## Wayfinding operations

Used by `/wayfinder`. The **map** is one issue, with **child** issues as its
tickets. Its labels are created when first needed: `gh label create <name>`
before the first issue that uses it.

- **Map**: an issue labeled `wayfinder:map`, holding the Notes, Decisions so
  far and Fog sections.
- **Child ticket**: an issue linked to the map as a sub-issue (above),
  labeled `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling`
  or `wayfinder:task`, and assigned to whoever claims it.
- **Blocking**: GitHub's issue dependencies:

  ```sh
  gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-database-id>
  ```

  The database id is `gh api repos/<owner>/<repo>/issues/<n> --jq .id`, not
  the `#number` or `node_id`. `issue_dependencies_summary.blocked_by` counts
  the open blockers. A ticket is unblocked when every blocker is closed.
- **Frontier**: the map's open children, without an open blocker or an
  assignee; the first in the map's order wins.
- **Claim**: `gh issue edit <n> --add-assignee @me`, before anything else.
- **Resolve**: `gh issue comment <n> --body "<answer>"`, then
  `gh issue close <n>`, then add a line with the gist and a link to the
  map's Decisions so far.
