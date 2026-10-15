# Shared helpers

Scripts source these with `. "$HELIXFORGE_LIB/common-script.sh"`; the engine
sets `HELIXFORGE_LIB` to this directory when it runs an entry. Scripts under
`../installers/` are referenced by id from catalog entries
(`[script] id = "name"` runs `installers/name.sh`); a script can also sit
beside its entry (`[script] path = "name.sh"`).
