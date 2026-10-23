// Desktop entries have a key-value escape layer in addition to Exec quoting.
// '%' is a field-code marker even inside quoted arguments.
fn exec_argument(value: &str) -> String {
    let mut quoted = String::from("\"");
    for character in value.chars() {
        match character {
            '%' => quoted.push_str("%%"),
            '\\' | '"' | '`' | '$' => {
                quoted.push('\\');
                quoted.push(character);
            }
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub fn command(executable: &str) -> String {
    let argument = exec_argument(executable);
    // GLib checks the executable before expanding %% field codes. env keeps
    // that lookup literal when the actual application path contains '%'.
    if executable.contains('%') {
        format!("/usr/bin/env -- {argument} --background")
    } else {
        format!("{argument} --background")
    }
}
