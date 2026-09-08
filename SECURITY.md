# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub Security Advisories at https://github.com/excelano/waddle/security/advisories/new. If you would rather not use GitHub, email david.anderson@excelano.com instead. I aim to respond within seven days.

Please do not open public issues for security problems.

## Supported versions

The latest 0.x release receives security fixes. Older versions are not supported.

## What waddle can access

waddle is a CLI that runs locally on your machine. It reads the DocLang, DocLang archive or docling JSON file you point it at (or standard input), plus the picture assets that file references beside it, refusing any asset path that leaves the input's own directory, and writes one new office package beside the input or at the path you give with `-o`. It never overwrites: a name that is taken gets a numbered one. `--dry-run` writes nothing. waddle makes no network calls of any kind, has no auth layer, and implements no administrative operations. It can only read and write files your operating-system user already has access to.

The `waddle-core` library reads and writes nothing; it takes a document in memory and returns bytes.

## What waddle stores

waddle stores nothing beyond the output you ask for. There is no config directory, no history file, no cache, no telemetry, no analytics, and no remote logging. It reads a document, writes a package, and exits.

## Verifying releases

Every GitHub release includes a `.sha256` file next to each archive listing its SHA-256 hash. Verify any download before running it:

    sha256sum waddle-x86_64-unknown-linux-gnu.tar.xz
    # compare against the value in waddle-x86_64-unknown-linux-gnu.tar.xz.sha256

Release artifacts are built by GitHub Actions from a tagged commit using the cargo-dist configuration in this repo (`dist-workspace.toml` and the generated `.github/workflows/release.yml`). The workflow and build configuration are public and auditable.
