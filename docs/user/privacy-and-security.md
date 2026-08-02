# Privacy and security

## What stays local

Tiamat is a Windows desktop app. Run state lives in a local SQLite database under the app data directory. Managed workspaces live on local disk. There is no cloud worker farm in v1.

## Secrets

- Likely credentials are redacted before persistence and UI emission.
- Secret-looking files are detected by path/pattern without reading values into prompts.
- Full environment blocks and secret-bearing command lines are never logged.
- Fixture secrets must never appear in repository artifacts, exports, or UI.

## Untrusted intake

Imported content is treated as untrusted project data. Prompt-injection text cannot expand write roots, disable tests/policy/cleanup, reveal credentials, or override Tiamat policy.

## Network and publishing

Default command policy denies arbitrary network publishing/deployment, credential-store access, force push, destructive reset of sources, and system configuration changes. Package restore/install inside managed roots follows the same process containment as tests.

## Data retention

Run metadata and redacted logs follow configurable retention. Managed workspaces with unpromoted work must not be deleted silently. Product policy retains `%APPDATA%\com.tiamat.desktop\tiamat\workspaces\`; unsigned-dev installer retain hooks may still be incomplete — see [install.md](install.md) and packaging docs.

## Related

[Containment limits](containment-limits.md) · [Known limits](known-limits.md)
