use crate::config::*;
use color_eyre::eyre::bail;
use color_eyre::Result;
use lazy_static::lazy_static;
use regex::Regex;
use std::io::{stderr, stdout, Write};
use std::process::{Command, Output};

pub struct BtrfsWrapper {
    chroot_to_host: bool,
}

impl Default for BtrfsWrapper {
    fn default() -> Self {
        BtrfsWrapper {
            chroot_to_host: true,
        }
    }
}

impl BtrfsWrapper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mv(&self, source: &str, target: &str) -> Result<Output> {
        self.run_command("mv", &[source, target])
    }

    pub fn subvolume_create(&self, path: &str) -> Result<Output> {
        self.run_command("btrfs", &["subvolume", "create", path])
    }

    pub fn subvolume_delete(&self, path: &str) -> Result<Output> {
        self.run_command("btrfs", &["subvolume", "delete", "--commit-after", path])
    }

    pub fn quota_enable(&self, path: &str) -> Result<Output> {
        self.run_command("btrfs", &["quota", "enable", path])
    }

    pub fn quota_rescan_wait(&self, path: &str) -> Result<Output> {
        self.run_command("btrfs", &["quota", "rescan", "-w", path])
    }

    pub fn qgroup_limit(&self, bytes: u64, path: &str) -> Result<Output> {
        self.run_command(
            "btrfs",
            &["qgroup", "limit", bytes.to_string().as_str(), path],
        )
    }

    pub fn qgroup_destroy(&self, qgroup: &str, path: &str) -> Result<Output> {
        self.run_command("btrfs", &["qgroup", "destroy", qgroup, path])
    }

    /// Returns the qgroup of a BTRFS subvolume located at `path`.
    pub fn get_qgroup(&self, path: &str) -> Result<String> {
        let output = String::from_utf8(self.qgroup_show_for(path)?.stdout)?;

        lazy_static! {
            static ref BTRFS_QGROUP_REGEX: Regex = Regex::new(r"^(\d+/\d+)\s").unwrap();
        }

        for line in output.split('\n') {
            println!("{}", line);
            if let Some(captures) = BTRFS_QGROUP_REGEX.captures(line) {
                if let Some(capture_match) = captures.get(1) {
                    return Ok(capture_match.as_str().to_owned());
                }
            }
        }

        bail!("Failed to get qgroup for {}", path);
    }

    fn qgroup_show_for(&self, path: &str) -> Result<Output> {
        self.run_command("btrfs", &["qgroup", "show", "-pcref", path])
    }

    /// Runs a command after eventually `chroot`ing into the host filesystem
    fn run_command(&self, command: &str, args: &[&str]) -> Result<Output> {
        fn run_prepared_command(command: &mut Command) -> Result<Output> {
            println!("Running: {:?}", command);

            let output = &command.output()?;

            stdout().write_all(&output.stdout)?;
            stderr().write_all(&output.stderr)?;

            Ok(output.clone())
        }

        if self.chroot_to_host {
            if let Ok(path) = std::env::var(HOST_FS_ENV_NAME) {
                return run_prepared_command(
                    Command::new("chroot")
                        .args(vec![path.as_str(), command])
                        .args(args),
                );
            }
        }

        let output = run_prepared_command(Command::new(command).args(args))?;

        if !&output.status.success() {
            bail!("`btrfs {}` failed: {}", &args.join(" "), &output.status);
        }

        Ok(output)
    }
}
