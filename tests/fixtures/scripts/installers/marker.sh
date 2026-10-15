#!/bin/sh -e
# Writes a marker where the test asks, using the shared helpers like a real installer.
. "$HELIXFORGE_LIB/common-script.sh"
command_exists sh
printf 'marker from %s\n' "$(pwd)" > "${HELIXFORGE_TEST_OUT:?}"
