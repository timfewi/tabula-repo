# Repository policy

Rule one: lean code, clear behavior and few moving parts. This tool creates and
checks portable context files; it does not own an agent loop, index or memory
service. Read README.md before changing the format or CLI.

Keep human explanations in ENTRY.md and derived freshness evidence in state.json.
No implicit project execution, network requests, recursive workspace scans or
credential reads. Preserve existing cards and unrelated work. Source and CLI
text are English. Follow normal Markdown link semantics.

Use Python standard library only unless a demonstrated requirement warrants a
dependency. Run project-check fast. Package builds and broader integration checks
are separate workloads justified by packaging or host changes; never activate a
host as a side effect. Keep a concise development checkpoint in README.md.

- Task-scoped staging is authorized. After review and required checks, commit
  the task changes and push to the current branch's existing upstream. Preserve
  unrelated work.
- Deployment, activation, installation, releases, visibility changes and history
  rewrites require separate authorization.
