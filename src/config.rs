//! Configuration types used to initialize the [`StreamContext`](crate::StreamContext).
//!
//! See the documentation of [`RuntimeConfig`] for more details.

use std::fmt::{Display, Formatter};
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

#[cfg(feature = "clap")]
use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::runner::spawn_remote_workers;
use crate::scheduler::HostId;
use crate::CoordUInt;

/// Environment variable set by the runner with the host id of the process. If it's missing the
/// process will have to spawn the processes by itself.
pub const HOST_ID_ENV_VAR: &str = "NOIR_HOST_ID";
/// Environment variable set by the runner with the content of the config file so that it's not
/// required to have it on all the hosts.
pub const CONFIG_ENV_VAR: &str = "NOIR_CONFIG";

/// The runtime configuration of the environment,
///
/// This configuration selects which runtime to use for this execution. The runtime is either local
/// (i.e. parallelism is achieved only using threads), or remote (i.e. using both threads locally
/// and remote workers).
///
/// In a remote execution the current binary is copied using scp to a remote host and then executed
/// using ssh. The configuration of the remote environment should be specified via a TOML
/// configuration file.
///
/// ## Local environment
///
/// ```
/// # use noir_compute::{StreamContext, RuntimeConfig};
/// let config = RuntimeConfig::local(1);
/// let env = StreamContext::new(config);
/// ```
///
/// ## Remote environment
///
/// ```
/// # use noir_compute::{StreamContext, RuntimeConfig};
/// # use std::fs::File;
/// # use std::io::Write;
/// let config = r#"
/// [[host]]
/// address = "host1"
/// base_port = 9500
/// num_cores = 16
///
/// [[host]]
/// address = "host2"
/// base_port = 9500
/// num_cores = 24
/// "#;
/// let mut file = File::create("config.toml").unwrap();
/// file.write_all(config.as_bytes());
///
/// let config = RuntimeConfig::remote("config.toml").expect("cannot read config file");
/// let env = StreamContext::new(config);
/// ```
///
/// ## From command line arguments
/// This reads from `std::env::args()` and reads the most common options (`--local`, `--remote`,
/// `--verbose`). All the unparsed options will be returned into `args`. You can use `--help` to see
/// their docs.
///
/// ```no_run
/// # use noir_compute::{RuntimeConfig, StreamContext};
/// let (config, args) = RuntimeConfig::from_args();
/// let env = StreamContext::new(config);
/// ```
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RuntimeConfig {
    /// Use only local threads.
    Local(LocalConfig),
    /// Use both local threads and remote workers.
    Remote(RemoteConfig),
}

// #[derive(Debug, Clone, Eq, PartialEq)]
// pub struct RuntimeConfig {
//     /// Which runtime to use for the environment.
//     pub runtime: RuntimeConfig,
//     /// In a remote execution this field represents the identifier of the host, i.e. the index
//     /// inside the host list in the config.
//     pub host_id: Option<HostId>,
// }

/// This environment uses only local threads.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LocalConfig {
    /// The number of CPU cores of this host.
    ///
    /// A thread will be spawned for each core, for each block in the job graph.
    pub num_cores: CoordUInt,
}

/// This environment uses local threads and remote hosts.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct RemoteConfig {
    /// The identifier for this host.
    #[serde(skip)]
    pub host_id: Option<HostId>,
    /// The set of remote hosts to use.
    #[serde(rename = "host")]
    pub hosts: Vec<HostConfig>,
    /// If specified some debug information will be stored inside this directory.
    pub tracing_dir: Option<PathBuf>,
    /// Remove remote binaries after execution
    #[serde(default)]
    pub cleanup_executable: bool,
}

/// The configuration of a single remote host.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct HostConfig {
    /// The IP address or domain name to use for connecting to this remote host.
    ///
    /// This must be reachable from all the hosts in the cluster.
    pub address: String,
    /// The first port to use for inter-host communication.
    ///
    /// This port and the following ones will be bound by the host, one for each connection between
    /// blocks of the job graph..
    pub base_port: u16,
    /// The number of cores of the remote host.
    ///
    /// This is the same as `LocalRuntimeConfig::num_cores`.
    pub num_cores: CoordUInt,
    /// The configuration to use to connect via SSH to the remote host.
    #[serde(default)]
    pub ssh: SSHConfig,
    /// If specified the remote worker will be spawned under `perf`, and its output will be stored
    /// at this location.
    pub perf_path: Option<PathBuf>,
}

/// The information used to connect to a remote host via SSH.
#[derive(Clone, Serialize, Deserialize, Derivative, Eq, PartialEq)]
#[derivative(Default)]
#[allow(clippy::upper_case_acronyms)]
pub struct SSHConfig {
    /// The SSH port this host listens to.
    #[derivative(Default(value = "22"))]
    #[serde(default = "ssh_default_port")]
    pub ssh_port: u16,
    /// The username of the remote host. Defaulted to the local username.
    pub username: Option<String>,
    /// The password of the remote host. If not specified ssh-agent will be used for the connection.
    pub password: Option<String>,
    /// The path to the private key to use for authenticating to the remote host.
    pub key_file: Option<PathBuf>,
    /// The passphrase for decrypting the private SSH key.
    pub key_passphrase: Option<String>,
}

impl std::fmt::Debug for SSHConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteHostSSHConfig")
            .field("ssh_port", &self.ssh_port)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "REDACTED"))
            .field("key_file", &self.key_file)
            .field(
                "key_passphrase",
                &self.key_passphrase.as_ref().map(|_| "REDACTED"),
            )
            .finish()
    }
}

#[cfg(feature = "clap")]
#[derive(Debug, Parser)]
#[clap(
    name = "noir",
    about = "Network of Operators In Rust",
    trailing_var_arg = true
)]
pub struct CommandLineOptions {
    /// Path to the configuration file for remote execution.
    ///
    /// When this is specified the execution will be remote. This conflicts with `--local`.
    #[clap(short, long)]
    remote: Option<PathBuf>,

    /// Number of cores in the local execution.
    ///
    /// When this is specified the execution will be local. This conflicts with `--remote`.
    #[clap(short, long)]
    local: Option<CoordUInt>,

    /// The rest of the arguments.
    args: Vec<String>,
}

impl RuntimeConfig {
    /// Build the configuration from the specified args list.
    #[cfg(feature = "clap")]
    pub fn from_args() -> (RuntimeConfig, Vec<String>) {
        let opt: CommandLineOptions = CommandLineOptions::parse();
        opt.validate();

        let mut args = opt.args;
        args.insert(0, std::env::args().next().unwrap());

        if let Some(num_cores) = opt.local {
            (Self::local(num_cores), args)
        } else if let Some(remote) = opt.remote {
            (Self::remote(remote).unwrap(), args)
        } else {
            unreachable!("Invalid configuration")
        }
    }

    /// Local environment that avoid using the network and runs concurrently using only threads.
    pub fn local(num_cores: CoordUInt) -> RuntimeConfig {
        RuntimeConfig::Local(LocalConfig { num_cores })
    }

    /// Remote environment based on the provided configuration file.
    ///
    /// The behaviour of this changes if this process is the "runner" process (ie the one that will
    /// execute via ssh the other workers) or a worker process.
    /// If it's the runner, the configuration file is read. If it's a worker, the configuration is
    /// read directly from the environment variable and not from the file (remote hosts may not have
    /// the configuration file).
    pub fn remote<P: AsRef<Path>>(config: P) -> Result<RuntimeConfig, ConfigError> {
        let mut config = if let Some(config) = RuntimeConfig::config_from_env() {
            config
        } else {
            log::info!("reading config from: {}", config.as_ref().display());
            let content = std::fs::read_to_string(config)?;
            toml::from_str(&content)?
        };

        // validate the configuration
        for (host_id, host) in config.hosts.iter().enumerate() {
            if host.ssh.password.is_some() && host.ssh.key_file.is_some() {
                return Err(ConfigError::Invalid(format!("Malformed configuration: cannot specify both password and key file on host {}: {}", host_id, host.address)));
            }
        }

        config.host_id = RuntimeConfig::host_id_from_env(config.hosts.len().try_into().unwrap());
        log::debug!("runtime configuration: {config:#?}");
        Ok(RuntimeConfig::Remote(config))
    }

    /// Extract the host id from the environment variable, if present.
    fn host_id_from_env(num_hosts: CoordUInt) -> Option<HostId> {
        let host_id = match std::env::var(HOST_ID_ENV_VAR) {
            Ok(host_id) => host_id,
            Err(_) => return None,
        };
        let host_id = match HostId::from_str(&host_id) {
            Ok(host_id) => host_id,
            Err(e) => panic!("Invalid value for environment {HOST_ID_ENV_VAR}: {e:?}"),
        };
        if host_id >= num_hosts {
            panic!(
                "Invalid value for environment {}: value too large, max possible is {}",
                HOST_ID_ENV_VAR,
                num_hosts - 1
            );
        }
        Some(host_id)
    }

    /// Extract the configuration from the environment, if it's present.
    fn config_from_env() -> Option<RemoteConfig> {
        match std::env::var(CONFIG_ENV_VAR) {
            Ok(config) => {
                info!("reading remote config from env {}", CONFIG_ENV_VAR);
                let config: RemoteConfig =
                    toml::from_str(&config).expect("Invalid configuration from environment");
                Some(config)
            }
            Err(_) => None,
        }
    }

    /// Spawn the remote workers via SSH and exit if this is the process that should spawn. If this
    /// is already a spawned process nothing is done.
    pub fn spawn_remote_workers(&self) {
        match &self {
            RuntimeConfig::Local(_) => {}
            #[cfg(feature = "ssh")]
            RuntimeConfig::Remote(remote) => {
                spawn_remote_workers(remote.clone());
            }
            #[cfg(not(feature = "ssh"))]
            RuntimeConfig::Remote(_) => {
                panic!("spawn_remote_workers() requires the `ssh` feature for remote configs.");
            }
        }
    }

    pub fn host_id(&self) -> Option<HostId> {
        match self {
            RuntimeConfig::Local(_) => Some(0),
            RuntimeConfig::Remote(remote) => remote.host_id,
        }
    }
}

impl Display for HostConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:{}-]", self.address, self.base_port)
    }
}

#[cfg(feature = "clap")]
impl CommandLineOptions {
    /// Check that the configuration provided is valid.
    fn validate(&self) {
        if !(self.remote.is_some() ^ self.local.is_some()) {
            panic!("Use one of --remote or --local");
        }
        if let Some(threads) = self.local {
            if threads == 0 {
                panic!("The number of cores should be positive");
            }
        }
    }
}

/// Default port for ssh, used by the serde default value.
fn ssh_default_port() -> u16 {
    22
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] toml::de::Error),

    #[error("Input-Output error: {0}")]
    IO(#[from] std::io::Error),

    #[error("Invalid configuration: {0}")]
    Invalid(String),
}
