#!/bin/sh -e

. ../../common-script.sh

. ../utility_functions.sh

createUser() {
    clear
    printf "%b\n" "${YELLOW}Create a new user${RC}"
    printf "%b\n" "${YELLOW}=================${RC}"
    printf "%b" "${YELLOW}Enter the username: ${RC}"
    read -r username

    if ! echo "$username" | grep '^[a-zA-Z]*$' > /dev/null; then
        printf "%b\n" "${RED}Username must only contain letters and cannot contain spaces.${RC}"
        exit 1
    fi

    # Not shown as it is typed, so it is not on the screen or in a saved log.
    printf "%b" "${YELLOW}Enter the password: ${RC}"
    stty -echo
    read -r password
    stty echo
    printf "\n"
    printf "%b" "${YELLOW}Enter the password again: ${RC}"
    stty -echo
    read -r password_confirmation
    stty echo
    printf "\n"

    if [ "$password" != "$password_confirmation" ]; then
        printf "%b\n" "${RED}Passwords do not match${RC}"
        exit 1
    fi

    "$ESCALATION_TOOL" useradd -m "$username" -g users -s /bin/bash
    echo "$username:$password" | "$ESCALATION_TOOL" chpasswd

    printf "%b\n" "${GREEN}User $username created successfully${RC}"
    printf "%b\n" "${GREEN}To add additional groups use Add User To Groups${RC}"
}

checkEnv
checkEscalationTool
createUser
