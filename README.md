<p align="center">
  <img src="assets/logo.png" alt="Lux Logo" width="160"/>
</p>

# lux

<p align="left">
  <a href="https://github.com/Lux-Python/Lux/releases"><img src="https://img.shields.io/badge/release-v0.1.0-blue.svg?style=flat-square" alt="Release" /></a>

  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-2021%20edition-orange.svg?style=flat-square&logo=rust" alt="Rust Edition" /></a>
  <a href="#safety"><img src="https://img.shields.io/badge/unsafe_code-deny-brightgreen.svg?style=flat-square" alt="Safe Rust" /></a>
  <a href="#performance"><img src="https://img.shields.io/badge/architecture-zero--copy%20CAS-blueviolet.svg?style=flat-square" alt="Architecture" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg?style=flat-square" alt="License" /></a>
</p>

A fast, memory-safe Python package and project manager written in safe Rust.

---

## Highlights

- Unified workflow for package management, virtual environments, lockfiles, and tools.
- Fast dependency resolution powered by an in-process PubGrub SAT solver.
- Comprehensive [project management](#projects) with a deterministic, multi-platform lockfile (`lux.lock`).
- **Binary ABI Inspection**: inspects native `.pyd`, `.so`, and `.dylib` extensions ([`lux doctor`](#binary-abi-inspection)) using in-process binary parsing.
- [Runs single-file scripts](#scripts) with [PEP 723 inline dependency metadata](#scripts).
- [Manages Python versions](#python-versions) with version pinning (`.python-version`).
- [Executes and installs CLI tools](#tools) in isolated environments (`lux tool` / `luxx`).
- Drop-in [pip-compatible interface](#the-pip-interface) with support for `-r requirements.txt` and `-e <path>` editable installs.
- Cargo-style [monorepo workspaces](#monorepo-workspaces) (`[tool.lux.workspace]`).
- Lockfile export ([`lux export`](#lockfile-export)) with cryptographic SHA-256 integrity hashes.
- Disk-space efficient [Content-Addressable Storage (CAS)](#cache-architecture) with atomic writes and zero-cost hardlinking.
- 100% safe Rust (`#![deny(unsafe_code)]`) with zero warnings under `-D clippy::all -D clippy::pedantic -D clippy::nursery`.
- Cross-platform: macOS, Linux, and Windows.

---

## Installation

Install Lux with our standalone installers:

#### Windows (PowerShell)
```powershell
powershell -ExecutionPolicy ByPass -c "irm https://raw.githubusercontent.com/Lux-Python/Lux/main/install.ps1 | iex"
```

#### macOS and Linux
```bash
curl -LsSf https://raw.githubusercontent.com/Lux-Python/Lux/main/install.sh | sh
```

#### With Cargo
```bash
cargo install --locked lux_cli --bin lux
```

#### From PyPI
```bash
pip install lux-python
```

---

## Features

### Projects

Lux manages project dependencies, lockfiles, virtual environments, and build pipelines:

```console
$ lux init my-app
     Created `pyproject.toml` (my-app)
 Initialized virtual environment at .venv in 2.14ms

$ cd my-app

$ lux add fastapi uvicorn "pydantic>=2.0"
   Resolving dependencies via PubGrub SAT solver...
   + fastapi v0.110.0
   + uvicorn v0.28.0
   + pydantic v2.6.4
   Installed 3 package(s) in 14.12ms

$ lux run uvicorn main:app --reload
INFO:     Uvicorn running on http://127.0.0.1:8000 (Press CTRL+C to quit)

$ lux lock
  Reconciled lux.lock (3 packages locked) in 0.42ms

$ lux sync
Synchronized 3 package(s) in 0.85ms
```

### Virtual Environments (`lux venv`)

Create a standalone, standard PEP 405 virtual environment anywhere in milliseconds:

```console
# Create default .venv in the current directory
$ lux venv
       Using host Python at C:\Python312\python.exe
 Initialized virtual environment at .venv in 3.12ms
Activate with .venv\Scripts\activate

# Or specify a custom directory name and Python version
$ lux venv myenv --python 3.12
```

### Scripts

Lux manages dependencies and environments for single-file Python scripts using PEP 723 inline metadata:

```console
$ cat << 'EOF' > script.py
# /// script
# dependencies = ["httpx", "rich"]
# ///
import httpx
from rich import print

resp = httpx.get("https://httpbin.org/get")
print(resp.json())
EOF

$ lux run script.py
   Resolving script dependencies...
   Installed 2 package(s) in 9.21ms
{
    'headers': {
        'Host': 'httpbin.org',
        'User-Agent': 'python-httpx/0.27.0'
    },
    'origin': '192.0.2.1',
    'url': 'https://httpbin.org/get'
}
```

### Tools

Lux executes and installs command-line tools provided by Python packages in hermetic, isolated environments (similar to `pipx`).

Run a tool ephemerally using `luxx` (or `lux tool run`):

```console
$ luxx ruff check .
All checks passed!

$ luxx ipython
Python 3.12.3 (main, Apr 15 2024, 18:20:11)
Type 'copyright', 'credits' or 'license' for more information
IPython 8.24.0 -- An enhanced Interactive Python. Type '?' for help.
In [1]:
```

Install a tool globally into `~/.lux/bin`:

```console
$ lux tool install black
   Installed black v24.4.2 in 6.41ms
   Installed 1 executable: black

$ black --version
black, 24.4.2 (compiled: yes)

$ lux tool list
black v24.4.2 (~/.lux/tools/black)
ruff  v0.4.4  (~/.lux/tools/ruff)
```

### Python Versions

Lux installs standalone, portable CPython interpreters and allows instant version pinning:

```console
$ lux python install 3.11 3.12 3.13
  Downloaded cpython-3.11.9
  Downloaded cpython-3.12.3
  Downloaded cpython-3.13.0rc1
   Installed 3 runtime(s) in 842ms

$ lux python list
   Available Python runtimes:
     cpython-3.11.9   (~/.lux/python/cpython-3.11.9-x86_64)
     cpython-3.12.3   (~/.lux/python/cpython-3.12.3-x86_64) [active]
     cpython-3.13.0   (~/.lux/python/cpython-3.13.0-x86_64)

$ lux python pin 3.12
      Pinned `.python-version` to `3.12`
```

### The `pip` Interface

Lux provides a drop-in replacement for `pip`, `pip-tools`, and `virtualenv` workflows with a 10–100x speedup:

```console
# Compile loose requirements into locked requirements
$ lux pip compile requirements.in -o requirements.txt
    Compiled 42 requirement(s) -> requirements.txt in 11.42ms

# Install packages into the active environment
$ lux pip install requests flask
   Resolving dependencies via PubGrub SAT solver...
   + requests v2.31.0
   + flask v3.0.3
   Installed 2 package(s) in 8.19ms

# Install from a requirements file
$ lux pip install -r requirements.txt
   Installed 42 package(s) in 24.50ms

# Install a local package in editable mode
$ lux pip install -e .
   + my-app v0.1.0 (editable)
   Installed 1 package(s) in 2.11ms

# Inspect installed distributions
$ lux pip list
Package                        Version        
------------------------------ ---------------
fastapi                        0.110.0        
pydantic                       2.6.4          
uvicorn                        0.28.0         
```

### Binary ABI Inspection (`lux doctor`)

The Subsystem Delta inspects native C/C++ and Rust extensions (`.pyd`, `.so`, `.dylib`) within `.venv` using in-process binary inspection (`object` crate) to catch missing DLLs, undefined symbols, and ABI mismatches before runtime:

```console
$ lux doctor
   Inspecting virtual environment at .venv...
   Discovered 14 binary extension(s)
       Checked numpy.core._multiarray_umath.pyd (PE x86_64) -> OK
       Checked cryptography.hazmat.bindings._rust.pyd (PE x86_64) -> OK
       Checked scipy._lib._ccallback_c.pyd (PE x86_64) -> OK
   Environment healthy: 0 missing symbols, 0 broken links.
```

### Monorepo Workspaces

Declare workspace members in the root `pyproject.toml`:

```toml
[tool.lux.workspace]
members = [
    "packages/*",
    "apps/backend",
    "apps/worker",
]
```

Synchronize the entire workspace into a shared, unified development environment:

```console
$ lux sync
   + mono-core v1.0.0 (workspace)
   + mono-web v2.0.0 (workspace)
Synchronized 2 workspace package(s) in 8.31ms
```

### Lockfile Export

Export locked dependencies from `lux.lock` into standard pip-compatible requirements files with cryptographic SHA-256 hashes:

```console
$ lux export --output requirements.txt
    Exported 42 package(s) -> requirements.txt in 2.10ms

$ cat requirements.txt
# This file was autogenerated by Lux via `lux export`.
# Deterministic dependency closure with cryptographic integrity verification.

requests==2.31.0 \
    --hash=sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
urllib3==2.0.7 \
    --hash=sha256:1b9a9d20c5d26ff38a4d46c4f039a8db4d0f62b1b369c3a078ec39fb7cb6a0df
```

---

## Performance & Architecture

Lux is designed for high throughput across all stages of package management:

- **Asynchronous HTTP/2 Client**: Streams project metadata from PEP 691 Simple endpoints, utilizing connection pooling and lazy wheel central directory range reads (PEP 658) to avoid downloading full archives when reading metadata.
- **In-Process SAT Solver**: Powered by a PubGrub dependency solver that operates over version sets directly in memory without subprocess overhead.
- **Content-Addressable Storage (CAS)**: Packages are stored once by their SHA-256 cryptographic digest and projected into virtual environments using atomic filesystem hardlinks, avoiding duplicate disk consumption.

---

## Safety & Security

- **Zero Unsafe:** `#![deny(unsafe_code)]` is strictly enforced across the entire codebase.
- **Zero Warnings:** Clean compilation under `-D clippy::all -D clippy::pedantic -D clippy::nursery`.
- **Integrity Verification:** Every cached artifact is verified against its SHA-256 cryptographic digest before linking.
- **Hermetic Isolation:** Virtual environments do not inherit system site-packages by default.

---

## Shell Completions

Generate shell completions for your shell:

```bash
# PowerShell
lux completions powershell >> $PROFILE

# Bash
lux completions bash > ~/.bash_completion

# Zsh
lux completions zsh > ~/.zfunc/_lux

# Fish
lux completions fish > ~/.config/fish/completions/lux.fish
```

---

## License

Lux is dual-licensed under:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

<p align="center">
  <sub>Engineered with safe Rust • Zero Unsafe</sub>
</p>
