#!/bin/sh -e

. ../common-script.sh
# Function to display the menu
printf "%b\n" "${YELLOW}Ensuring OpenSSL is installed...${RC}"

# Install OpenSSL
if ! command_exists openssl; then
    case "$PACKAGER" in
        pacman)
            "$ESCALATION_TOOL" "$PACKAGER" -S --noconfirm --needed openssl
            ;;
        apt-get|nala|dnf|eopkg)
            "$ESCALATION_TOOL" "$PACKAGER" install -y openssl
            ;;
        zypper)
            "$ESCALATION_TOOL" "$PACKAGER" install openssl
            ;;
        apk)
            "$ESCALATION_TOOL" "$PACKAGER" add openssl
            ;;
        xbps-install)
            "$ESCALATION_TOOL" "$PACKAGER" -Sy openssl
            ;;
        *)
            printf "%b\n" "${RED}Unsupported package manager: ""$PACKAGER""${RC}"
            exit 1
            ;;
    esac
fi

# Files this tool encrypts start with this line, so decryption knows their
# PBKDF2 work factor; a file without it was encrypted by an earlier version,
# with OpenSSL's default of 10,000 iterations, and still opens.
ITERATIONS=600000
ENC_HEADER="Helix Toolbox enc pbkdf2 $ITERATIONS"

# Read a password into the variable named $1 without showing it, so it is not
# on the screen or in a saved transcript.
read_secret() {
    printf "%b" "$2"
    stty -echo 2>/dev/null || true
    trap 'stty echo 2>/dev/null' EXIT
    trap 'stty echo 2>/dev/null; exit 130' INT TERM
    read -r "$1"
    stty echo 2>/dev/null || true
    trap - EXIT INT TERM
    printf "\n"
}

# The password goes to OpenSSL on its standard input or another descriptor,
# never on its command line, where any local user could read it.
encrypt_one() {
    {
        printf '%s\n' "$ENC_HEADER"
        printf '%s\n' "$PASSWORD" | openssl enc -aes-256-cbc -salt -pbkdf2 -iter "$ITERATIONS" -pass stdin -in "$1"
    } > "$2"
}

decrypt_one() {
    if [ "$(head -c "${#ENC_HEADER}" "$1" | tr -d '\000')" = "$ENC_HEADER" ]; then
        printf '%s\n' "$PASSWORD" | {
            tail -c +"$((${#ENC_HEADER} + 2))" "$1" | openssl enc -aes-256-cbc -d -pbkdf2 -iter "$ITERATIONS" -pass fd:3 -out "$2"
        } 3<&0
    else
        printf '%s\n' "$PASSWORD" | openssl enc -aes-256-cbc -d -pbkdf2 -pass stdin -in "$1" -out "$2"
    fi
}

show_menu() {
    printf "%b\n" "========================================================"
    printf "%b\n" " File/Directory Encryption/Decryption"
    printf "%b\n" "========================================================"
    printf "%b\n" "How to use:-"
    printf "%b\n" "if you encrypt or decrypt a file include new file name for successful operation"
    printf "%b\n" "if you encrypt or decrypt a folder include new directory name for successful operation"
    printf "%b\n" "========================================================"
    printf "%b\n" "1. Encrypt a file or directory"
    printf "%b\n" "2. Decrypt a file or directory"
    printf "%b\n" "3. Exit"
    printf "%b\n" "========================================================"
}

# Function to encrypt a file
encrypt_file() {
    printf "%b" "Enter the path to the file or directory to encrypt: "
    read -r INPUT_PATH

    if [ ! -e "$INPUT_PATH" ]; then
        printf "%b\n" "Path does not exist!"
        return
    fi

    printf "%b" "Enter the path for the encrypted file or directory: "
    read -r OUTPUT_PATH

    read_secret PASSWORD "Enter the encryption password: "
    read_secret PASSWORD_CONFIRM "Enter the password again: "
    if [ "$PASSWORD" != "$PASSWORD_CONFIRM" ]; then
        unset PASSWORD PASSWORD_CONFIRM
        printf "%b\n" "Passwords do not match."
        return
    fi
    unset PASSWORD_CONFIRM

    if [ -d "$INPUT_PATH" ]; then
        # Encrypt each file in the directory
        find "$INPUT_PATH" -type f | while read -r FILE; do
            REL_PATH="${FILE#"$INPUT_PATH"/}"
            OUTPUT_FILE="$OUTPUT_PATH/$REL_PATH.enc"
            mkdir -p "$(dirname "$OUTPUT_FILE")"

            if encrypt_one "$FILE" "$OUTPUT_FILE"; then
                printf "%b\n" "Encrypted: $OUTPUT_FILE"
            else
                printf "%b\n" "Failed to encrypt: $FILE"
            fi
        done
    else
        # Encrypt a single file
        if [ -d "$OUTPUT_PATH" ]; then
            printf "%b\n" "Output path must be a file for single file encryption."
            return
        fi
        mkdir -p "$(dirname "$OUTPUT_PATH")"

        if encrypt_one "$INPUT_PATH" "$OUTPUT_PATH"; then
            printf "%b\n" "Encrypted: $OUTPUT_PATH"
        else
            printf "%b\n" "Failed to encrypt: $INPUT_PATH"
        fi
    fi
    unset PASSWORD
}

# Function to decrypt a file
decrypt_file() {
    printf "%b" "Enter the path to the file or directory to decrypt: "
    read -r INPUT_PATH

    if [ ! -e "$INPUT_PATH" ]; then
        printf "%b\n" "Path does not exist!"
        return
    fi

    printf "%b" "Enter the path for the decrypted file or directory: "
    read -r OUTPUT_PATH

    read_secret PASSWORD "Enter the decryption password: "

    if [ -d "$INPUT_PATH" ]; then
        # Decrypt each file in the directory
        find "$INPUT_PATH" -type f -name '*.enc' | while read -r FILE; do
            REL_PATH="${FILE#"$INPUT_PATH"/}"
            OUTPUT_FILE="$OUTPUT_PATH/${REL_PATH%.enc}"
            mkdir -p "$(dirname "$OUTPUT_FILE")"

            if decrypt_one "$FILE" "$OUTPUT_FILE"; then
                printf "%b\n" "Decrypted: $OUTPUT_FILE"
            else
                printf "%b\n" "Failed to decrypt: $FILE"
            fi
        done
    else
        # Decrypt a single file
        if [ -d "$OUTPUT_PATH" ]; then
            printf "%b\n" "Output path must be a file for single file decryption."
            return
        fi
        mkdir -p "$(dirname "$OUTPUT_PATH")"

        if decrypt_one "$INPUT_PATH" "$OUTPUT_PATH"; then
            printf "%b\n" "Decrypted: $OUTPUT_PATH"
        else
            printf "%b\n" "Failed to decrypt: $INPUT_PATH"
        fi
    fi
    unset PASSWORD
}

main(){
    clear
    while true; do
        show_menu
        printf "%b" "Enter your choice: "
        read -r CHOICE

        case $CHOICE in
            1) encrypt_file ;;
            2) decrypt_file ;;
            3) printf "%b\n" "Exiting..."; exit 0 ;;
            *) printf "%b\n" "Invalid choice. Please try again." ;;
        esac

        printf "%b\n" "Press [Enter] to continue..."
        read -r _
    done
}

checkEnv
checkEscalationTool
main
