#!/bin/sh -e
# Writes a marker where the test asks, using the shared helpers like a real installer.
# shellcheck source=tests/fixtures/scripts/shared/common-script.sh
. "$HELIXFORGE_LIB/common-script.sh"
command_exists sh
printf 'marker from %s\n' "$(pwd)" > "${HELIXFORGE_TEST_OUT:?}"
