#!/bin/sh -e
# Ignores SIGHUP, as some long-running tools do, and records the child's process id.
trap '' HUP
echo "stubborn started"
sh -c 'echo $$ > "${HELIXFORGE_TEST_OUT:?}.stubborn"; exec sleep 300'
