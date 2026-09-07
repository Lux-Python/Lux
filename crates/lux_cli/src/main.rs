#![deny(unsafe_code)]

use std::path::PathBuf;
use clap::{CommandFactory, Parser, Subcommand};
use miette::Result;

use lux_cli::commands::{
    run_add, run_build, run_cache, run_completions, run_doctor, run_export, run_init,
    run_pip_compile, run_pip_freeze, run_pip_install, run_pip_list, run_pip_uninstall,
    run_publish, run_python_find, run_python_install, run_python_list, run_python_pin,
    run_remove, run_resolve, run_run, run_sync, run_tool_install, run_tool_list, run_tool_run,
    run_tool_uninstall, run_venv,
};

#[derive(Parser, Debug)]
#[command(
    name = "lux",
    version,
    about = "Fast Python package manager and resolver",
    long_about = "Lux is a fast, memory-safe Python package manager and resolver written in Rust."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize a new Python project with pyproject.toml and .venv
    Init {
        /// Target directory path (defaults to current directory)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Explicit project name
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Create a new Python virtual environment
    Venv {
        /// Directory path to create the virtual environment in (defaults to .venv)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Python interpreter or version to use (e.g. `3.12`)
        #[arg(short, long, value_name = "PYTHON")]
        python: Option<String>,
    },

    /// Add dependencies to pyproject.toml, resolve, and install into .venv
    Add {
        /// Package specification(s) to add (e.g. `requests>=2.31.0`)
        #[arg(value_name = "PACKAGES")]
        packages: Vec<String>,

        /// Install a local project in editable mode
        #[arg(short = 'e', long = "editable", value_name = "PATH")]
        editable: Option<PathBuf>,

        /// Do not synchronize the virtual environment after updating the manifest
        #[arg(long)]
        no_sync: bool,
    },

    /// Remove dependencies from pyproject.toml, lockfile, and virtual environment
    Remove {
        /// Package name(s) to remove
        #[arg(required = true, value_name = "PACKAGES")]
        packages: Vec<String>,
    },

    /// Reconcile and synchronize the virtual environment against lux.lock
    Sync,

    /// Resolve dependencies via PubGrub-CDCL SAT solver without altering environment
    Resolve {
        /// Optional packages to resolve; if omitted, resolves pyproject.toml
        #[arg(value_name = "PACKAGES")]
        packages: Vec<String>,

        /// Render resolution graph as a formatted dependency tree
        #[arg(short, long)]
        tree: bool,
    },

    /// Display hierarchical project dependency tree
    Tree,

    /// Execute a command within the context of the hermetic virtual environment
    Run {
        /// Executable to launch (e.g. `python`, `pytest`, `uvicorn`)
        #[arg(required = true)]
        command: String,

        /// Arguments passed through to the child command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Inspect or clean the Content-Addressable Storage (CAS) cache
    Cache {
        /// Cache operation: `dir`, `info`, or `clean`
        #[arg(value_name = "ACTION")]
        action: Option<String>,
    },

    /// Diagnose virtual environment health and binary ABI dependencies (.pyd, .so, .dylib)
    Doctor,

    /// Build project into a wheel distribution via isolated PEP 517 build backend
    Build {
        /// Target output directory (defaults to dist/)
        #[arg(short, long, value_name = "DIR")]
        out_dir: Option<PathBuf>,

        /// Build source distribution (.tar.gz)
        #[arg(long)]
        sdist: bool,

        /// Build wheel distribution (.whl)
        #[arg(long)]
        wheel: bool,
    },

    /// Manage Python interpreters and versions
    Python {
        #[command(subcommand)]
        command: PythonCommands,
    },

    /// Run or install isolated CLI tools (ruff, black, etc.)
    Tool {
        #[command(subcommand)]
        command: ToolCommands,
    },

    /// Drop-in pip compatibility interface (install, compile, list, freeze, uninstall)
    Pip {
        #[command(subcommand)]
        command: PipCommands,
    },

    /// Publish distribution artifacts to `PyPI` or custom index
    Publish {
        /// Distribution files to upload (defaults to dist/*)
        #[arg(value_name = "FILES")]
        files: Vec<PathBuf>,

        /// Custom package repository URL
        #[arg(short, long)]
        repository: Option<String>,

        /// API upload authentication token
        #[arg(short, long)]
        token: Option<String>,
    },

    /// Export lockfile to alternative format (e.g. requirements.txt)
    Export {
        /// Export format (defaults to requirements-txt)
        #[arg(short, long, default_value = "requirements-txt")]
        format: String,

        /// Output file destination (defaults to stdout)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,

        /// Exclude cryptographic hashes from output
        #[arg(long)]
        no_hashes: bool,
    },

    /// Generate shell completion script for the specified shell
    Completions {
        /// Target shell (bash, zsh, fish, powershell, elvish)
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
enum PythonCommands {
    /// List installed and available Python runtimes
    List {
        /// Show available remote standalone distributions
        #[arg(short, long)]
        all: bool,
    },
    /// Pin Python version for current directory (.python-version)
    Pin {
        /// Python version to pin (e.g. 3.12)
        version: String,
    },
    /// Locate Python executable matching version
    Find {
        /// Version prefix to find (e.g. 3.12)
        version: Option<String>,
    },
    /// Install standalone Python distribution(s)
    Install {
        /// Versions to install (e.g. 3.12 3.13)
        #[arg(required = true)]
        versions: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ToolCommands {
    /// Run an isolated tool on-the-fly
    Run {
        /// Tool executable name
        tool: String,
        /// Arguments passed to the tool
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Install an isolated tool globally
    Install {
        /// Tool name to install
        tool: String,
    },
    /// List installed global tools
    List,
    /// Uninstall a global tool
    Uninstall {
        /// Tool name to remove
        tool: String,
    },
}

#[derive(Subcommand, Debug)]
enum PipCommands {
    /// Install packages into active virtual environment
    Install {
        /// Package specification(s) to install
        #[arg(value_name = "PACKAGES")]
        packages: Vec<String>,

        /// Install from the given requirements file
        #[arg(short = 'r', long = "requirement", value_name = "FILE")]
        requirements: Vec<PathBuf>,

        /// Install a project in editable mode from the given local path
        #[arg(short = 'e', long = "editable", value_name = "PATH")]
        editables: Vec<PathBuf>,
    },
    /// Compile requirements.in to locked requirements.txt
    Compile {
        /// Input requirements.in file
        file: PathBuf,
        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// List installed packages in active environment
    List,
    /// Freeze installed packages in pip format
    Freeze,
    /// Uninstall packages from active environment
    Uninstall {
        /// Package names to uninstall
        #[arg(required = true)]
        packages: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { path, name }) => run_init(path, name),
        Some(Commands::Venv { path, python }) => run_venv(path, python),
        Some(Commands::Add { packages, editable, no_sync }) => {
            run_add(&packages, editable.as_deref(), no_sync).await
        }
        Some(Commands::Remove { packages }) => run_remove(&packages),
        Some(Commands::Sync) => run_sync(),
        Some(Commands::Resolve { packages, tree }) => run_resolve(&packages, tree),
        Some(Commands::Tree) => run_resolve(&[], true),
        Some(Commands::Run { command, args }) => run_run(&command, &args),
        Some(Commands::Cache { action }) => run_cache(action.as_deref()),
        Some(Commands::Doctor) => run_doctor(),
        Some(Commands::Build { out_dir, sdist, wheel }) => run_build(out_dir, sdist, wheel).await,
        Some(Commands::Export { format, output, no_hashes }) => {
            run_export(&format, output, no_hashes)
        }
        Some(Commands::Completions { shell }) => {
            let mut cmd = Cli::command();
            run_completions(shell, &mut cmd)
        }
        Some(Commands::Python { command }) => match command {
            PythonCommands::List { all } => run_python_list(all),
            PythonCommands::Pin { version } => run_python_pin(&version),
            PythonCommands::Find { version } => run_python_find(version.as_deref()),
            PythonCommands::Install { versions } => run_python_install(&versions),
        },
        Some(Commands::Tool { command }) => match command {
            ToolCommands::Run { tool, args } => run_tool_run(&tool, &args),
            ToolCommands::Install { tool } => run_tool_install(&tool),
            ToolCommands::List => run_tool_list(),
            ToolCommands::Uninstall { tool } => run_tool_uninstall(&tool),
        },
        Some(Commands::Pip { command }) => match command {
            PipCommands::Install { packages, requirements, editables } => {
                run_pip_install(&packages, &requirements, &editables).await
            }
            PipCommands::Compile { file, output } => run_pip_compile(&file, output),
            PipCommands::List => run_pip_list(),
            PipCommands::Freeze => run_pip_freeze(),
            PipCommands::Uninstall { packages } => run_pip_uninstall(&packages),
        },
        Some(Commands::Publish { files, repository, token }) => {
            run_publish(&files, repository.as_deref(), token.as_deref()).await
        }
        None => {
            let mut cmd = Cli::command();
            let _ = cmd.print_help();
            println!();
            Ok(())
        }
    }
}
