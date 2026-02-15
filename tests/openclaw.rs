use cronclaw::openclaw;
use std::path::Path;

#[test]
fn build_command_has_correct_args() {
    let cmd = openclaw::build_command("pro-worker", "analyse this data", Path::new("/tmp/ws"), 300);
    let prog = cmd.get_program();
    let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();

    assert_eq!(prog, "openclaw");
    assert_eq!(
        args,
        &[
            "agent",
            "--message",
            "analyse this data",
            "--to",
            "pro-worker",
            "--local",
            "--timeout",
            "300",
        ]
    );
}

#[test]
fn build_command_sets_working_directory() {
    let cmd = openclaw::build_command("worker", "do stuff", Path::new("/my/workspace"), 60);
    assert_eq!(cmd.get_current_dir(), Some(Path::new("/my/workspace")));
}

#[test]
fn build_command_handles_multiline_prompt() {
    let prompt = "Line one\nLine two\nLine three";
    let cmd = openclaw::build_command("agent", prompt, Path::new("/tmp"), 300);
    let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();

    // The full multiline prompt should be passed as a single argument
    assert_eq!(args[1], "--message");
    assert_eq!(args[2], prompt);
}

#[test]
fn build_command_handles_special_characters_in_prompt() {
    let prompt = r#"Analyse "this" & that's $data"#;
    let cmd = openclaw::build_command("agent", prompt, Path::new("/tmp"), 300);
    let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
    assert_eq!(args[2], prompt);
}

#[test]
fn build_command_passes_timeout() {
    let cmd = openclaw::build_command("agent", "hello", Path::new("/tmp"), 3600);
    let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
    assert_eq!(args[6], "--timeout");
    assert_eq!(args[7], "3600");
}
