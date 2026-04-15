# tsx-rename-component-vs-new-consumer

A renames the `OkButton` React component project-wide to `PrimaryButton`, updating its declaration in `components.tsx`, the named import in `App.tsx`, and the JSX usage. B adds a new `CancelRow` component in `App.tsx` that renders `<OkButton label="Cancel" />`.

After merge the new JSX must use `<PrimaryButton>`. Exercises project-scope symbol rewriting through a TSX payload (named import + JSX identifier) while keeping the module filename `components` stable.
