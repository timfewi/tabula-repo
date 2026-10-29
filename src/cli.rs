use crate::*;
use std::{io::Read, path::PathBuf};
const COMMANDS: &[&str] = &[
    "init",
    "update",
    "check",
    "doctor",
    "context",
    "test-snapshot",
    "import-test",
];
fn help(command: &str) -> &'static str {
    match command {
        "init" => include_str!("help/init.txt"),
        "update" => include_str!("help/update.txt"),
        "check" => include_str!("help/check.txt"),
        "doctor" => include_str!("help/doctor.txt"),
        "context" => include_str!("help/context.txt"),
        "test-snapshot" => include_str!("help/test-snapshot.txt"),
        "import-test" => include_str!("help/import-test.txt"),
        _ => include_str!("help/main.txt"),
    }
}
fn usage(command: &str) -> String {
    help(command).split("\n\n").next().unwrap().to_string() + "\n"
}
fn parse_error(command: &str, message: &str) -> (i32, String, String) {
    (
        2,
        String::new(),
        format!(
            "{}tabula{}: error: {message}\n",
            usage(command),
            if command.is_empty() {
                String::new()
            } else {
                format!(" {command}")
            }
        ),
    )
}
fn integer(text: &str) -> Option<i64> {
    let text = text.trim_matches(compat::is_space);
    let (negative, digits) = if let Some(t) = text.strip_prefix('-') {
        (true, t)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let mut value = 0i64;
    let mut previous_digit = false;
    let mut count = 0;
    for c in digits.chars() {
        if c == '_' {
            if !previous_digit {
                return None;
            }
            previous_digit = false;
            continue;
        }
        let d = compat::decimal(c)?;
        count += 1;
        value = value.saturating_mul(10).saturating_add(d as i64);
        previous_digit = true;
    }
    if count == 0 || !previous_digit {
        None
    } else {
        Some(if negative { -value } else { value })
    }
}
fn optional(text: &str) -> bool {
    text.starts_with('-')
        && text != "-"
        && !regex::Regex::new(r"^-[0-9]+(?:\.[0-9]*)?$")
            .unwrap()
            .is_match(text)
}
pub fn run(args: &[String], input: &[u8]) -> (i32, String, String) {
    let Some(command) = args.first().map(String::as_str) else {
        return parse_error("", "the following arguments are required: command");
    };
    if command.starts_with("--") && command.len() > 2 && "--version".starts_with(command) {
        return (0, "Tabula 0.1.0\n".into(), String::new());
    }
    if command == "-h"
        || (command.starts_with("--") && command.len() > 2 && "--help".starts_with(command))
    {
        return (0, help("").into(), String::new());
    }
    if optional(command) {
        return parse_error("", "the following arguments are required: command");
    }
    if !COMMANDS.contains(&command) {
        return parse_error(
            "",
            &format!(
                "argument command: invalid choice: {} (choose from {})",
                compat::repr(&json!(command)),
                COMMANDS
                    .iter()
                    .map(|x| format!("'{x}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    let mut root = None;
    let mut purpose = None;
    let mut files = vec![];
    let mut review = vec![];
    let mut upgrade = false;
    let mut as_json = false;
    let mut max_bytes = None;
    let mut unknown = vec![];
    let mut i = 1;
    let mut positional = false;
    while i < args.len() {
        let arg = &args[i];
        if !positional && (arg == "--help" || arg == "-h") {
            return (0, help(command).into(), String::new());
        }
        if !positional && arg == "--" {
            positional = true;
            i += 1;
            continue;
        }
        let (option, inline) = if !positional {
            arg.split_once('=')
                .map(|(a, b)| (a, Some(b.to_string())))
                .unwrap_or((arg, None))
        } else {
            (arg.as_str(), None)
        };
        let mut options = vec!["--help", "--json"];
        match command {
            "init" => options.push("--purpose"),
            "update" => options.extend(["--upgrade", "--review"]),
            "context" => options.extend(["--files", "--max-bytes"]),
            "test-snapshot" => options.push("--files"),
            _ => {}
        }
        let matches: Vec<_> = options
            .iter()
            .filter(|x| option.starts_with("--") && x.starts_with(option))
            .copied()
            .collect();
        let option = if !positional && matches.len() == 1 {
            matches[0]
        } else {
            option
        };
        if !positional && matches.len() > 1 {
            return parse_error(
                command,
                &format!(
                    "ambiguous option: {option} could match {}",
                    matches.join(", ")
                ),
            );
        }
        if !positional && ["--help", "--json", "--upgrade"].contains(&option) {
            if let Some(value) = &inline {
                return parse_error(
                    command,
                    &format!(
                        "argument {option}: ignored explicit argument {}",
                        compat::repr(&json!(value))
                    ),
                );
            }
            if option == "--help" {
                return (0, help(command).into(), String::new());
            }
        }
        if !positional && option == "--json" {
            as_json = true;
        } else if !positional && option == "--upgrade" && command == "update" {
            upgrade = true;
        } else if !positional
            && ((option == "--purpose" && command == "init")
                || (option == "--max-bytes" && command == "context"))
        {
            let value = if let Some(value) = inline {
                value
            } else {
                if i + 1 >= args.len() || optional(&args[i + 1]) {
                    return parse_error(
                        command,
                        &format!("argument {option}: expected one argument"),
                    );
                }
                i += 1;
                args[i].clone()
            };
            if option == "--purpose" {
                purpose = Some(value);
            } else {
                match integer(&value) {
                    Some(n) => max_bytes = Some(n),
                    None => {
                        return parse_error(
                            command,
                            &format!(
                                "argument --max-bytes: invalid int value: {}",
                                compat::repr(&json!(value))
                            ),
                        )
                    }
                }
            }
        } else if !positional
            && ((option == "--files" && ["context", "test-snapshot"].contains(&command))
                || (option == "--review" && command == "update"))
        {
            let mut values = vec![];
            if let Some(value) = inline {
                values.push(value);
            } else {
                while i + 1 < args.len() && !optional(&args[i + 1]) {
                    i += 1;
                    values.push(args[i].clone());
                }
            }
            if values.is_empty() {
                return parse_error(
                    command,
                    &format!("argument {option}: expected at least one argument"),
                );
            }
            if option == "--files" {
                files = values;
            } else {
                review = values;
            }
        } else if !positional && optional(arg) {
            unknown.push(arg.clone());
        } else if root.is_none() {
            root = Some(arg.clone());
        } else {
            unknown.push(arg.clone());
        }
        i += 1;
    }
    let mut required = vec![];
    if command == "init" && purpose.is_none() {
        required.push("--purpose");
    }
    if ["context", "test-snapshot"].contains(&command) && files.is_empty() {
        required.push("--files");
    }
    if command == "context" && max_bytes.is_none() {
        required.push("--max-bytes");
    }
    if !required.is_empty() {
        return parse_error(
            command,
            &format!(
                "the following arguments are required: {}",
                required.join(", ")
            ),
        );
    }
    if !unknown.is_empty() {
        return parse_error(
            "",
            &format!("unrecognized arguments: {}", unknown.join(" ")),
        );
    }
    let result = (|| -> Result<V> {
        let root = PathBuf::from(root.as_deref().unwrap_or(".")).canonicalize()?;
        if !root.is_dir() {
            return Err(Error::new("repository root must be a directory"));
        }
        match command {
            "init" => operations::initialize(&root, purpose.as_deref().unwrap()),
            "update" => operations::update(&root, upgrade, &review),
            "check" => operations::check(&root),
            "doctor" => operations::doctor(&root),
            "context" => context::context(&root, &files, max_bytes.unwrap(), as_json),
            "test-snapshot" => evidence::test_snapshot(&root, &files),
            "import-test" => evidence::import_test(&root, input),
            _ => unreachable!(),
        }
    })();
    let result = match result {
        Ok(v) => v,
        Err(e) => {
            let mut v = json!({"status":"error"});
            for (k, x) in entries(&e.diagnostic(None, None)) {
                v[k] = x.clone();
            }
            v
        }
    };
    let code = match s(&result["status"]) {
        "error" => 2,
        "stale" => 1,
        _ => 0,
    };
    let output = context::render(&result, as_json);
    if command == "context" && max_bytes.is_some_and(|n| n <= 0 || output.len() > n as usize) {
        return (
            2,
            String::new(),
            "tabula: byte budget cannot fit response; increase --max-bytes\n".into(),
        );
    }
    (code, output, String::new())
}
pub fn main() -> i32 {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut input = vec![];
    if args.first().is_some_and(|x| x == "import-test") {
        if let Err(e) = std::io::stdin().take(16001).read_to_end(&mut input) {
            eprintln!("tabula: {e}");
            return 2;
        }
    }
    let (code, out, err) = run(&args, &input);
    use std::io::Write;
    let _ = std::io::stdout().write_all(out.as_bytes());
    let _ = std::io::stderr().write_all(err.as_bytes());
    code
}
