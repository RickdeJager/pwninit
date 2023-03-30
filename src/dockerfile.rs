/// Helper functions to pull ld and libc files from a docker container.
use colored::Colorize;
use regex::Regex;

use snafu::Snafu;

use std::path::Path;
use std::process::Command;
use std::collections::HashMap;


#[derive(Debug, Snafu)]
#[allow(clippy::enum_variant_names)]

pub enum Error {
    #[snafu(display("Docker run error: {}", message))]
    Run { message: String },

    #[snafu(display("Failed to parse dockerfile: {}", message))]
    Parse { message: String },
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn scan_dockerfile(dockerfile: &Path) -> Result<String> {
    // Read the dockerfile from the given path
    let dockerfile_contents = std::fs::read_to_string(dockerfile).expect("failed to read dockerfile");
    
    let docker_tag = dockerfile_contents
        .lines()
        .rev() // We want to find the last FROM statement
        .find(|line| line.to_uppercase().starts_with("FROM"))
        .ok_or(Error::Parse { message: "failed to find FROM statement in dockerfile".to_string() })?
        .split_whitespace()
        .nth(1)
        .ok_or(Error::Parse { message: "failed to find tag in FROM".to_string() })?;
    Ok(docker_tag.to_string())
}

// TODO: This implicitly assumes that each name is unique, which is not necessarily true.
//       For example, if you install 32 bit libraries, you will end up with multiple libc versions.
//
//       In a docker base image, this may not be an issue. This assumption is fine for images like
//       ubuntu and archlinux.
fn parse_ldconfig(content: &str) -> HashMap<&str, &str> {
    let r = Regex::new(r"\s+(?P<name>[\w.\-]+)\s\(.+\) => (?P<path>[\w/.\-]+)$").unwrap();
    let mut paths = HashMap::new();
    for line in content.lines() {
        if let Some(caps) = r.captures(line) {
            let lib = caps.name("name").unwrap().as_str();
            let path = caps.name("path").unwrap().as_str();
            paths.insert(lib, path);
        }
    }
    paths
}

fn check_docker_stderr(proc: &std::process::Output) -> Result<()> {
    // If the exit code is zero, we can return Ok(())
    if proc.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8(proc.stderr.clone()).unwrap();
    if !stderr.is_empty() {
        // Return a run snafu here
        return Err(Error::Run {
            message: stderr,
        });
    }
    Ok(())
}

fn docker_get_paths(tag: &str, wanted: Vec<&str>) -> Result<Vec<String>> {
    let proc = Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg(tag)
        .arg("ldconfig")
        .arg("-p")
        .output()
        .expect("failed to execute docker create");

    check_docker_stderr(&proc)?;
    let output = String::from_utf8(proc.stdout).expect("failed to parse docker output");
    let paths = parse_ldconfig(&output);

    Ok(wanted
        .iter()
        .map(|lib| paths.get(lib))
        .filter_map(|p| p.map(|p| p.to_string()))
        .collect())
}

fn docker_copy_file(container_id: &str, path: &str) -> Result<()> {
    let copy_proc = Command::new("docker")
        .arg("cp")
        .arg("-L")
        .arg(format!("{}:{}", container_id, path))
        .arg(".")
        .output()
        .expect("failed to execute docker cp");

    check_docker_stderr(&copy_proc)?;

    println!(
        "{}",
        format!(
            "Extracted {} from the docker container.",
            path.bold(),
        ).green()
    );

    Ok(())
}

fn extract_docker_libc_ld(tag: &str, container_id: &str) -> Result<()> {
    let paths = docker_get_paths(&tag, vec!["libc.so.6", "ld-linux-x86-64.so.2"])?;

    // Copy the files we _know_ to be in the container.
    for path in &paths {
        docker_copy_file(&container_id, path)?
    }

    // Add some more paths manually, since we can't get alpine paths from ldconfig for example.
    let additional_paths = vec![
        "/lib/ld-musl-x86_64.so.1",
        "/lib/libc.musl-x86_64.so.1",
    ];

    additional_paths.iter().for_each(|path| {
        let _ = docker_copy_file(&container_id, path);
    });

    Ok(())
}

pub fn download_libc_ld_for_docker_tag(tag: &str) -> Result<()> {
    // TODO; I'm just assuming we can run docker as user,
    // i.e. we are in the docker group

    println!(
        "{}",
        format!(
            "Creating a container for tag: {}\nThis may take a while...",
            tag.bold()
        ).green()
    );
    
    // We can only copy files from containers, so let's spin up a temp container for the given
    // image tag. This command will return the container id.
    let container_proc = Command::new("docker")
        .arg("create")
        .arg(tag)
        .output()
        .expect("failed to execute docker create");

    check_docker_stderr(&container_proc)?;

    let container_id = String::from_utf8(container_proc.stdout).unwrap().trim().to_string();
    let res = extract_docker_libc_ld(&tag, &container_id);

    // Clean up the container we just created, even though some of the previous operations may have
    // failed.
    let _rm_proc = Command::new("docker")
        .arg("rm")
        .arg(container_id)
        .output()
        .expect("failed to execute docker rm");

    // Return the result of the extract operation
    res
}
