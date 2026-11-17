//! `netsmith import case place`: a case model's angles and offset for a pose given as a matrix (#757).

fn netsmith(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn case_place_prints_the_lines_of_a_case_model_for_a_matrix() {
    // A box drawn upside down that the board's bottom face rests in: half a turn about x, down
    // by the board's thickness.
    let out = netsmith(&["import", "case", "place", "1 0 0 0", "0 -1 0 0", "0 0 -1 -1.6"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        ".rx &= 180\n.ry &= 0\n.rz &= 0\n.x &= 0mm\n.y &= 0mm\n.z &= -1.6mm\n"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("checked:") && err.contains("within 0"), "{err}");
    // A quarter turn about two axes, pasted as a 4 x 4 matrix.
    let out = netsmith(&[
        "import", "case", "place",
        "[[0, 0, 1, 10], [1, 0, 0, 5], [0, 1, 0, 0], [0, 0, 0, 1]]",
    ]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        ".rx &= 90\n.ry &= 0\n.rz &= 90\n.x &= 10mm\n.y &= 5mm\n.z &= 0mm\n"
    );
}

#[test]
fn case_place_refuses_a_mirror_and_a_matrix_of_the_wrong_size() {
    let out = netsmith(&["import", "case", "place", "1 0 0 0  0 -1 0 0  0 0 1 0"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("mirror"), "{err}");
    let out = netsmith(&["import", "case", "place", "1", "0", "0"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("3 were given"));
}
