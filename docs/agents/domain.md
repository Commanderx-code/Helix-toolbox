# Domain docs

How the skills read this repository's domain documentation while exploring
the code. The repository is single-context: one glossary and one set of
decision records.

## Before exploring, read these

- **`GLOSSARY.md`** at the repository root.
- **`docs/adr/`**: the decision records that touch the area about to change.

Neither exists yet. If they are missing, carry on without them and do not
suggest creating them; `/domain-modeling` creates them when a term or a
decision is first settled.

## Use the glossary's vocabulary

Name a domain concept in an issue title, a proposal, a hypothesis or a test
name with the term `GLOSSARY.md` gives it, not a synonym the glossary avoids.
A concept the glossary lacks is either language the project does not use,
or a gap to note for `/domain-modeling`.

## Flag decision conflicts

When a change contradicts a decision record, say so rather than overriding
it silently:

> _Contradicts ADR-0007 (the catalog's TOML format), but worth reopening
> because…_
