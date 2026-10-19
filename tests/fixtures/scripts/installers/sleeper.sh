#!/bin/sh -e
# Runs a long foreground child and records its process id beside HELIX_TOOLBOX_TEST_OUT.
echo "sleeper started"
sh -c 'echo $$ > "${HELIX_TOOLBOX_TEST_OUT:?}.child"; exec sleep 300'
echo "sleeper finished"
