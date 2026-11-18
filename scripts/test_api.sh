#!/usr/bin/env bash
set -euo pipefail

BASE_URL="http://127.0.0.1:24568"
FILE_PATH="${1:-/c/test/document.docx}"
OUTPUT_FOLDER="${2:-/c/output}"

curl -sS "$BASE_URL/health"
echo

curl -sS "$BASE_URL/api/check-wps"
echo

curl -sS -X POST "$BASE_URL/api/convert" \
  -F "file=@${FILE_PATH}" \
  -F "output_name=output.pdf"
echo

curl -sS -X POST "$BASE_URL/api/convert-batch?async=true" \
  -F "files=@${FILE_PATH}" \
  -F "output_folder=${OUTPUT_FOLDER}"
echo
