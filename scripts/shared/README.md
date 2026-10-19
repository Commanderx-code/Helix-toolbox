# Shared helpers

Scripts source these with `. "$HELIX_TOOLBOX_LIB/common-script.sh"`; the
engine sets `HELIX_TOOLBOX_LIB` to this directory when it runs an entry, and
`HELIXFORGE_LIB`, its name before 1.7, for the installers that use it. Scripts under
`../installers/` are referenced by id from catalog entries
(`[script] id = "name"` runs `installers/name.sh`); a script can also sit
beside its entry (`[script] path = "name.sh"`).
