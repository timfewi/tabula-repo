# Security

Tabula reads local files referenced by a guide. It is not a sandbox against
another process changing the filesystem concurrently. Normal CLI operations do
not fetch remote links or execute repository code.

Keep secrets, authentication stores, private logs and personal information out of
guides and linked files. Review what you share or publish independently.

A private vulnerability-reporting channel must be configured before public
release. Until one is advertised on the official project page, do not post
sensitive reproductions in public issues. Use synthetic fixtures in ordinary
bug reports and never include tokens, private source or personal paths.

No supported-release window is promised before the first public release.
