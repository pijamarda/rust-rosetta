extern crate difference;
extern crate docopt;
extern crate hyper;
extern crate term;
extern crate regex;
extern crate rust_rosetta;
extern crate rustc_serialize;
extern crate walkdir;

use rust_rosetta::rosetta_code::find_unimplemented_tasks::all_tasks;

use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;

use docopt::Docopt;
use difference::Difference;
use hyper::Client;
use hyper::header::Connection;
use regex::Regex;
use term::Terminal;
use walkdir::WalkDir;

const USAGE: &'static str = r"
Detect unimplemented tasks.

This script prints out the name of each task, followed by whether it is implemented online,
locally, or both.

If no tasks are specified, determines the status for all tasks.

Optionally prints out a diff as well.

Usage:
    coverage [options] [--localonly | --remoteonly | --unimplemented] [<tasks>...]

Options:
    -h --help           Show this screen.
    --nodiff            Don't print diffs.
    --localonly         Only print tasks that are implemented locally, but not on the wiki.
    --remoteonly        Only print tasks that are implemented on the wiki, but not locally.
    --unimplemented     Only print tasks that neither implemented locally nor remotely.
";

#[derive(Debug, RustcDecodable)]
struct Args {
    arg_tasks: Vec<String>,
    flag_nodiff: bool,
    flag_localonly: bool,
    flag_remoteonly: bool,
    flag_unimplemented: bool,
}

/// Transforms a task title to a URL task title.
fn normalize(title: &str) -> String {
    title.replace(" ", "_")
}

fn main() {
    let args: Args = Docopt::new(USAGE)
                         .and_then(|d| d.decode())
                         .unwrap_or_else(|e| e.exit());

    let mut t = term::stdout().unwrap();

    let http_client = Client::new();

    // Determine which tasks are implemented locally by walking the src folder and reading the
    // comment at the top of the file.
    let mut local_tasks: HashMap<String, PathBuf> = HashMap::new();
    let task_comment = Regex::new("// http://rosettacode.org/wiki/(.+)").unwrap();
    for entry in WalkDir::new("../src") {
        let entry = entry.unwrap();
        let lib_or_mod = Regex::new("^lib|mod$").unwrap();
        let file_stem = entry.path().file_stem().unwrap().to_str().unwrap();

        // If we find a non-Rust file (or a lib or mod) skip it.
        match entry.path().extension().and_then(|s| s.to_str()) {
            Some("rs") if !lib_or_mod.is_match(file_stem) => (),
            _ => continue,
        }

        let file = File::open(entry.path()).unwrap();
        let first_line = BufReader::new(file).lines().next().unwrap().unwrap();
        let task_name = task_comment.captures(&first_line)
                                    .and_then(|c| c.at(1))
                                    .expect(&format!("could not parse task name for {:?}",
                                                     entry.path()));

        local_tasks.insert(task_name.to_owned(), entry.path().to_owned());
    }

    let task_titles: Vec<String> = if args.arg_tasks.len() > 0 {
        args.arg_tasks
    } else {
        all_tasks().iter().cloned().map(|t| t.title).collect()
    };

    // Extracts the code from the first <lang rust> tag
    let rust_re = Regex::new(r"==\{\{header\|Rust\}\}==(?s:.*?)<lang rust>((?s:.*?))</lang>")
                      .unwrap();

    for title in task_titles {
        let task_url = &format!("http://rosettacode.org/wiki/{}", normalize(&title));

        let local_code = local_tasks.get(&normalize(&title))
                                    .and_then(|path| File::open(path).ok())
                                    .and_then(|mut file| {
                                        let mut local_code = String::new();
                                        file.read_to_string(&mut local_code).unwrap();
                                        Some(local_code)
                                    });


        let mut res = http_client.get(&format!("{}?{}", task_url, "action=raw"))
                                 .header(Connection::close())
                                 .send()
                                 .unwrap();

        let mut body = String::new();
        res.read_to_string(&mut body).unwrap();
        let online_code = rust_re.captures(&body)
                                 .and_then(|captures| Some(captures.at(1).unwrap()));

        if args.flag_localonly && !(local_code.is_some() && online_code.is_none()) {
            continue;
        }

        if args.flag_remoteonly && !(local_code.is_none() && online_code.is_some()) {
            continue;
        }

        if args.flag_unimplemented && (local_code.is_some() || online_code.is_some()) {
            continue;
        }

        t.attr(term::Attr::Bold).unwrap();
        writeln!(t, "{}", title).unwrap();
        t.reset().unwrap();

        writeln!(t,
                 "Local: {}, Remote: {}",
                 local_code.is_some(),
                 online_code.is_some())
            .unwrap();

        if !args.flag_nodiff && online_code.is_some() && local_code.is_some() {
            let (_dist, changeset) = difference::diff(&online_code.unwrap(),
                                                      &local_code.unwrap(),
                                                      "\n");

            let mut t = term::stdout().unwrap();

            for i in 0..changeset.len() {
                match changeset[i] {
                    Difference::Same(ref x) => {
                        t.reset().unwrap();
                        writeln!(t, " {}", x).unwrap();
                    }
                    Difference::Add(ref x) => {
                        t.fg(term::color::GREEN).unwrap();
                        for line in x.split("\n") {
                            writeln!(t, "+{}", line).unwrap();
                        }
                    }
                    Difference::Rem(ref x) => {
                        t.fg(term::color::RED).unwrap();
                        for line in x.split("\n") {
                            writeln!(t, "-{}", line).unwrap();
                        }
                    }
                }
            }
        }
        t.reset().unwrap();
        t.flush().unwrap();
    }
}
