fn error_for(account_id: u64) -> String {
    // returns an error for the user_id field
    let raw = r#"raw user_id literal"#;
    let _ = raw;
    format!("Log field 'user_id' failed for {}", account_id)
}
