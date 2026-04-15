# py-rename-module-attr-vs-caller
A renames DEFAULT_TIMEOUT -> TIMEOUT_SECONDS. B adds double_timeout()
referencing the old name. diff3 silently merges; runtime NameError.
