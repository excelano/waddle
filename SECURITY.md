# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately, using the **Report a vulnerability** button on the Security tab of this repository. If that button is not there, or you would rather not go through GitHub, email david.anderson@excelano.com instead. I aim to respond within seven days.

Please do not open public issues for security problems.

## Supported versions

The latest release receives security fixes. Older versions are not supported. A fix ships in a new release; update the way you installed.

## What waddle can access

waddle is a CLI that runs locally on your machine. It reads the DocLang, DocLang archive or docling JSON file you point it at (or standard input), plus the picture assets that file references beside it, refusing any asset path that leaves the input's own directory, and writes one new office package beside the input or at the path you give with `-o`. It never overwrites: a name that is taken gets a numbered one. `--dry-run` writes nothing. waddle makes no network calls of any kind, has no auth layer, and implements no administrative operations. It can only read and write files your operating-system user already has access to.

The `waddle-core` library reads and writes nothing; it takes a document in memory and returns bytes.

## What waddle stores

waddle stores nothing beyond the output you ask for. There is no config directory, no history file, no cache, no telemetry, no analytics, and no remote logging. It reads a document, writes a package, and exits.

The one exception is the Claude Code skill: `waddle --install-skill` writes it to `~/.claude/skills/waddle/`, and `waddle --uninstall-skill` removes that directory. Neither runs unless you ask for it.

## Verifying releases

Every GitHub release carries the `.deb` packages and a Windows zip, built by `build-release`, the release program in excelano/shipping, from a tagged commit. The Excelano apt repository is signed, so `apt` verifies each package it installs, and WinGet checks the zip against the SHA-256 recorded in its manifest. To check a file downloaded from the release page by hand, compare its `sha256sum` with the digest GitHub lists for that asset.
