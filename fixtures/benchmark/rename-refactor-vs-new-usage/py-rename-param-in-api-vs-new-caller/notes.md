# py-rename-param-in-api-vs-new-caller
A renames the retries parameter to max_attempts across fetch. B adds a new
fetch_many caller that still uses the old parameter name retries. Proper
merge must propagate the rename into B's new function.
