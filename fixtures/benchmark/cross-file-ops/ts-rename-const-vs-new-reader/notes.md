# ts-rename-const-vs-new-reader

A project-renames a constant `API_URL → BASE_URL` across `config.ts` and `client.ts`. B adds a second reader `healthUrl()` in `client.ts` that interpolates `${API_URL}` inside a template literal.

After merge the new reader must reference `BASE_URL`, both at the anchor needle (`return fetch(API_URL);` in B's log) and inside the template literal of the inserted body. Tests rename-through-template-literal.
