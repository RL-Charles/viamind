//! Native writes never reuse a predictable temporary or clobber an unapproved destination.
use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};

use viamind_doc::{Project, file};

#[test]
fn unique_temporaries_preserve_unrelated_files_and_cleanup_after_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("board.viamind.json");
    let old_temporary = directory.path().join("board.viamind.json.tmp");
    fs::write(&old_temporary, "unrelated").unwrap();
    let project = Project::new("atomic");
    file::save(&project, &path).unwrap();
    assert_eq!(file::load(&path).unwrap(), project);
    assert_eq!(fs::read_to_string(&old_temporary).unwrap(), "unrelated");
    let existing = fs::read(&path).unwrap();
    assert!(file::save_new(&Project::new("replacement"), &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), existing);
    let blocked = directory.path().join("directory");
    fs::create_dir(&blocked).unwrap();
    assert!(file::save(&project, &blocked).is_err());
    assert!(blocked.is_dir());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 3, "no temporary files left after success or failure");
}

#[test]
fn simultaneous_no_clobber_saves_have_exactly_one_complete_winner() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("race.viamind.json");
    let barrier = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|i| {
            let path = path.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let project = Project::new(format!("writer {i}"));
                barrier.wait();
                file::save_new(&project, path).ok().map(|()| project)
            })
        })
        .collect();
    let winners: Vec<_> = workers.into_iter().filter_map(|worker| worker.join().unwrap()).collect();
    assert_eq!(winners.len(), 1);
    assert_eq!(file::load(path).unwrap(), winners[0]);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
