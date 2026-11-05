# Settings migration plan

KodAct's settings move from one flat `settings.json` to a versioned schema with
scopes (user, project, local). Nothing you set today is lost: the migration
reads the old file once, writes the new one beside it, and keeps a backup.

## What changes

| Setting | Today | After |
|:--|:--|:--|
| `theme` | `"dark"` or `"light"` | `{ "mode": "system", "accent": "blue" }` |
| `model` | a model name | the endpoint profile's name |
| `fonts` | not stored | a list, read from the `fonts` folder |
| `permissions` | one list | allow / ask / deny, per scope |

## Steps

1. Read `settings.json` and validate it against the v1 schema.
2. Map each field to its v2 home, using the table above.
3. Write `settings.v2.json`, then rename it into place in one step.
4. Keep the old file as `settings.v1.backup.json` for a release.

```rust
pub fn migrate(v1: &V1) -> Result<V2, MigrateError> {
    let theme = Theme::from_legacy(&v1.theme)?;
    Ok(V2 { version: 2, theme, ..V2::default() })
}
```

## Checklist

- [x] Inventory every field the app reads today
- [x] Write the v2 schema and its defaults
- [ ] Round-trip test: v1 → v2 → read back
- [ ] Test on Windows with a read-only settings file

> If the old file fails validation, KodAct keeps using it and says so once,
> rather than guessing.

## Risks

- **A half-written file.** Writing beside the old one and renaming avoids it.
- **Older KodAct versions.** They keep reading v1 until the backup is removed.
