/// Controls the verbosity of output during a mutation test run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    /// Only progress lines.
    Quiet,
    /// Show extra detail: skip messages, test output on SURVIVED and ERROR.
    Normal,
    /// Show all detail including baseline and every mutation's test output.
    Debug,
}

impl Verbosity {
    pub fn from_count(count: u8) -> Self {
        match count {
            0 => Verbosity::Quiet,
            1 => Verbosity::Normal,
            _ => Verbosity::Debug,
        }
    }

    pub fn show_detail(self) -> bool {
        matches!(self, Verbosity::Normal | Verbosity::Debug)
    }

    pub fn show_always(self) -> bool {
        matches!(self, Verbosity::Debug)
    }
}

/// Configuration for a mutation testing run.
pub struct Config<'a> {
    /// Path to the Ruby project root.
    pub project_path: &'a str,
    /// Optional shell command to run the test suite.
    pub test_cmd: Option<&'a str>,
    /// Optional path to a cache file for incremental runs.
    pub cache_path: Option<&'a str>,
    /// Optional limit on the number of mutations to test.
    pub limit: Option<usize>,
    /// Whether to only list mutation points without running tests.
    pub list_only: bool,
    /// Dump the concrete syntax tree with node IDs for every parsed file, then exit.
    pub dump_cst: bool,
    /// How verbose the output should be.
    pub verbosity: Verbosity,
}
