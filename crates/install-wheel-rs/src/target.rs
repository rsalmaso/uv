use std::path::PathBuf;

/// A target environment into which a wheel can be installed.
pub struct Target {
    /// The Python interpreter, as returned by `sys.executable`.
    pub sys_executable: PathBuf,
    /// The `purelib` directory, as returned by `sysconfig.get_paths()`.
    pub purelib: PathBuf,
    /// The `platlib` directory, as returned by `sysconfig.get_paths()`.
    pub platlib: PathBuf,
    /// The `scripts` directory, as returned by `sysconfig.get_paths()`.
    pub include: PathBuf,
    /// The `scripts` directory, as returned by `sysconfig.get_paths()`.
    pub scripts: PathBuf,
    /// The `data` directory, as returned by `sysconfig.get_paths()`.
    pub data: PathBuf,
    /// The Python version, as returned by `sys.version_info`.
    pub python_version: (u8, u8),
}
