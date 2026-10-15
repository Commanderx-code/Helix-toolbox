#!/bin/sh -e
# Runs a long foreground child and records its process id beside HELIXFORGE_TEST_OUT.
echo "sleeper started"
sh -c 'echo $$ > "${HELIXFORGE_TEST_OUT:?}.child"; exec sleep 300'
echo "sleeper finished"
