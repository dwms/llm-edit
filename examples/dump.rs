use llm_edit::{apply_log, load_tree, ops::parse_log};
use std::path::Path;
fn main() -> anyhow::Result<()> {
    for (case, agent) in [("fixtures/py/rename-vs-new-use","agent_b"),("fixtures/py/extract-helper-vs-bugfix","agent_b")] {
        let mut tree = load_tree(Path::new(&format!("{}/base", case)))?;
        let log = std::fs::read_to_string(format!("{}/{}/.llm-edit/log.jsonl", case, agent))?;
        let ops = parse_log(&log)?;
        apply_log(&mut tree, &ops)?;
        for (p, t) in &tree {
            let d = Path::new("/tmp/llm-edit-dump").join(case).join(agent);
            std::fs::create_dir_all(&d)?;
            std::fs::write(d.join(p), t)?;
        }
    }
    Ok(())
}
