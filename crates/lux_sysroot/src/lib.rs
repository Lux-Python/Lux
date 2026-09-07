//! `lux_sysroot`: Binary ABI inspector, sysroot overlay, and PEP 517 build runner.


#![deny(unsafe_code)]

pub mod abi;
pub mod builder;
pub mod overlay;

pub use abi::{AbiError, AbiInspector, BinaryAbiReport, BinaryFormat, DynamicDependency};
pub use builder::{BuilderError, Pep517BuildConfig, Pep517Builder};
pub use overlay::{SysrootFlags, SysrootOverlay};
