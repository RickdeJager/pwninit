use colored::Colorize;

use std::path::Path;
use std::process::Command;

pub fn scan_dockerfile(dockerfile: &Path) -> Result<String, String> {
    // Read the dockerfile from the given path
    let dockerfile_contents = std::fs::read_to_string(dockerfile).expect("failed to read dockerfile");
    
    let docker_tag = dockerfile_contents
        .lines()
        .rev() // We want to find the last FROM statement
        .find(|line| line.starts_with("FROM"))
        .expect("failed to find FROM in dockerfile")
        .split_whitespace()
        .nth(1)
        .expect("failed to find tag in FROM");
    Ok(docker_tag.to_string())
}

pub fn download_libc_ld_for_docker_tag(tag: &str) -> Result<(), String> {
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

    if !container_proc.status.success() {
        // TODO: print error here
        return Err("failed to execute docker create".to_string());
    }

    let container_id = String::from_utf8(container_proc.stdout).unwrap().trim().to_string();

    // Now we can copy the linker and libc from the container.
    // TODO: For now I'm assuming them to be at /lib/ld-linux-x86-64.so.2 and /lib/x86_64-linux-gnu/libc.so.6
    //
    //       The correct way to do this to run docker exec in the container and run ldconfig -p
    //          ldconfig -p | grep ld-
    //          ldconfig -p | grep libc.so.6
    //       The current implementation will not work for arch or alpine images, for example.
    //
    //       docker exec 5a90d014148d bash -c 'ldconfig -p | grep libc.so.6'
    //
    // TODO: Docker cp apparently doesn't throw proper exit codes, so we want to read stdout/err
    // here and check for errors manually.
    let copy_proc_ld = Command::new("docker")
        .arg("cp")
        .arg("-L")
        .arg(format!("{}:/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2", container_id))
        .arg(".")
        .output();

    let copy_proc_libc = Command::new("docker")
        .arg("cp")
        .arg("-L")
        .arg(format!("{}:/lib/x86_64-linux-gnu/libc.so.6", container_id))
        .arg(".")
        .output();

    // Clean up the container we just created
    let _rm_proc = Command::new("docker")
        .arg("rm")
        .arg(container_id)
        .output()
        .expect("failed to execute docker rm");

    let copy_proc_ld = copy_proc_ld.expect("failed to execute docker cp for ld");
    if !copy_proc_ld.status.success() {
        return Err("failed to execute docker cp for ld".to_string());
    }

    let copy_proc_libc = copy_proc_libc.expect("failed to execute docker cp for libc");
    if !copy_proc_libc.status.success() {
        return Err("failed to execute docker cp for libc".to_string());
    }

    Ok(())
}
