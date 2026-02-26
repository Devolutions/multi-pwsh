# Setup PowerShell Core Action

A local GitHub Action to download and install any version of **PowerShell Core** on GitHub Actions runners.

This action is based on [mchave3/setup-pwsh](https://github.com/mchave3/setup-pwsh) and has been imported locally for customization and to eliminate external dependencies.

## Features

- ✅ **Cross-platform**: Works on Windows, macOS, and Linux runners
- ✅ **Flexible versioning**: Install latest stable, preview, or specific versions
- ✅ **Multi-architecture**: Supports x64, x86, ARM64, and ARM32
- ✅ **Caching**: Uses runner tool cache for faster subsequent runs
- ✅ **Automatic detection**: Auto-detects OS and architecture

## Usage

### Basic Usage - Latest Version

```yaml
steps:
  - uses: actions/checkout@v4

  - name: Setup PowerShell
    uses: ./.github/actions/setup-pwsh

  - name: Run PowerShell script
    shell: pwsh
    run: |
      $PSVersionTable
```

### Install LTS/Stable Version (7.4.x)

```yaml
steps:
  - name: Setup PowerShell LTS
    uses: ./.github/actions/setup-pwsh
    with:
      version: 'stable'
```

### Install Specific Version

```yaml
steps:
  - name: Setup PowerShell 7.4.6
    uses: ./.github/actions/setup-pwsh
    with:
      version: '7.4.6'
```

### Install Latest Preview

```yaml
steps:
  - name: Setup PowerShell Preview
    uses: ./.github/actions/setup-pwsh
    with:
      version: 'preview'
```

### Specify Architecture

```yaml
steps:
  - name: Setup PowerShell ARM64
    uses: ./.github/actions/setup-pwsh
    with:
      version: 'stable'
      architecture: 'arm64'
```

### Matrix Testing

```yaml
jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
        pwsh-version: ['7.2.0', '7.4.0', 'stable', 'preview']

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v4

      - name: Setup PowerShell
        uses: ./.github/actions/setup-pwsh
        with:
          version: ${{ matrix.pwsh-version }}

      - name: Test PowerShell
        shell: pwsh
        run: |
          Write-Host "PowerShell Version: $($PSVersionTable.PSVersion)"
          Write-Host "OS: $($PSVersionTable.OS)"
```

## Inputs

| Input | Description | Required | Default |
|-------|-------------|----------|---------|
| `version` | PowerShell version to install | No | `latest` |
| `architecture` | Target architecture | No | `auto` |
| `github-token` | GitHub token for API authentication (optional, recommended to avoid rate limits) | No | `""` |

### Version Options

| Value | Description |
|-------|-------------|
| `latest` | Latest release (currently 7.5.x) - from /releases/latest |
| `stable` or `lts` | Latest LTS release (currently 7.4.x - supported until Nov 2026) |
| `preview` | Latest preview/RC release |
| `7.5.4` | Specific version (e.g., 7.5.4, 7.4.6) |

### PowerShell Support Lifecycle

| Version | Type | End of Support |
|---------|------|----------------|
| 7.5.x | Current | May 2026 |
| 7.4.x | **LTS** | **Nov 2026** |
| 7.2.x | LTS (EOL) | ~~Nov 2024~~ |

> ⚠️ **Note**: PowerShell 7.2.x reached end-of-support in November 2024. Use `stable` or `lts` to get the current LTS version (7.4.x).

### Architecture Options

| Value | Description | Windows | macOS | Linux |
|-------|-------------|---------|-------|-------|
| `auto` | Auto-detect (default) | ✅ | ✅ | ✅ |
| `x64` | 64-bit Intel/AMD | ✅ | ✅ | ✅ |
| `x86` | 32-bit | ✅ | ❌ | ❌ |
| `arm64` | ARM 64-bit | ✅ | ✅ | ✅ |
| `arm32` | ARM 32-bit | ❌ | ❌ | ✅ |

## Outputs

| Output | Description |
|--------|-------------|
| `version` | The installed PowerShell version |
| `path` | The installation path |

### Using Outputs

```yaml
steps:
  - name: Setup PowerShell
    id: setup-pwsh
    uses: ./.github/actions/setup-pwsh
    with:
      version: 'stable'

  - name: Display installed version
    run: |
      echo "Installed version: ${{ steps.setup-pwsh.outputs.version }}"
      echo "Installation path: ${{ steps.setup-pwsh.outputs.path }}"
```

## How It Works

1. **Detects** the runner's operating system and architecture
2. **Fetches** release information from [PowerShell GitHub releases](https://github.com/PowerShell/PowerShell/releases)
3. **Downloads** the appropriate package (ZIP for Windows, tar.gz for Unix)
4. **Extracts** to the runner tool cache
5. **Adds** the installation path to `$PATH`

## Troubleshooting

### Rate Limiting

The `github-token` input is **optional but recommended** to avoid GitHub API rate limiting:

```yaml
steps:
  - name: Setup PowerShell
    uses: ./.github/actions/setup-pwsh
    with:
      version: 'stable'
      github-token: ${{ github.token }}  # Optional but recommended
```

> 💡 **Tip**: Without a token, anonymous API requests are limited to 60/hour. With `github.token`, you get 5,000/hour. The action will work without it, but you may hit rate limits in workflows that run frequently.

### Version Not Found

Ensure the version exists on [PowerShell releases](https://github.com/PowerShell/PowerShell/releases). Use exact version numbers like `7.4.0`, not `7.4`.

### Architecture Not Supported

Not all architectures are available for all platforms:
- `x86` is only available on Windows
- `arm32` is only available on Linux

## License

This action is based on [mchave3/setup-pwsh](https://github.com/mchave3/setup-pwsh) which is licensed under the MIT License.

Original Copyright (c) 2025 Mickaël CHAVE

Modified and maintained by AwakeCoding for the AwakeCoding.PSRemoting project.
