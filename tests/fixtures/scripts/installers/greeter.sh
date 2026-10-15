#!/bin/sh -e
# Reads one line from the terminal, as an interactive installer would.
printf 'Your name? '
read -r name
echo "Hello, $name."
