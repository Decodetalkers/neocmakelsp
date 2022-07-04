use anyhow::Result;
use lsp_types::{CompletionItem, CompletionItemKind};
use once_cell::sync::Lazy;
use std::iter::zip;
use std::process::Command;
pub static BUILD_COMMAND: Lazy<Result<Vec<CompletionItem>>> = Lazy::new(|| {
    let re = regex::Regex::new(r"[z-zA-z]+\n-+").unwrap();
    let output = Command::new("cmake")
        .arg("--help-commands")
        .output()?
        .stdout;
    let temp = String::from_utf8_lossy(&output);
    let key: Vec<_> = re
        .find_iter(&temp)
        .map(|message| message.as_str())
        .collect();
    let content: Vec<_> = re.split(&temp).into_iter().collect();
    let context = &content[1..];
    Ok(zip(key, context)
        .into_iter()
        .map(|(akey, message)| CompletionItem {
            label: akey.to_string(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some(message.to_string()),
            ..Default::default()
        })
        .collect())
});

#[cfg(test)]
mod tests {
    use std::iter::zip;
    #[test]
    fn tst_regex() {
        let re = regex::Regex::new(r"-+").unwrap();
        assert!(re.is_match("---------"));
        assert!(re.is_match("-------------------"));
        let temp = "javascrpt---------it is";
        let splits: Vec<_> = re.split(temp).into_iter().collect();
        let aftersplit = vec!["javascrpt", "it is"];
        for (split, after) in zip(splits, aftersplit) {
            assert_eq!(split, after);
        }
    }
    use std::process::Command;
    #[test]
    fn tst_cmakecommand_buildin() {
        let re = regex::Regex::new(r"[z-zA-z]+\n-+").unwrap();
        let output = Command::new("cmake")
            .arg("--help-commands")
            .output()
            .unwrap()
            .stdout;
        let temp = String::from_utf8_lossy(&output);
        let key: Vec<_> = re.find_iter(&temp).collect();
        let splits: Vec<_> = re.split(&temp).into_iter().collect();

        for akey in key {
            println!("{}", akey.as_str());
        }
        let newsplit = &splits[1..];
        for split in newsplit.iter() {
            println!("{split}");
        }
    }
}
