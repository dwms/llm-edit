# js-rename-fn-vs-add-call-in-third-file
A renames log -> writeLog across three JS files. B adds a new caller
doAudit() in consumer.js using the old name. Textual merge leaves doAudit
calling the nonexistent log().
