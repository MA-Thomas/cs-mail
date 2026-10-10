#!/usr/bin/env bash
# Reuses the clean enrollment/service setup; needs two empty local databases.
# Synthetic inbound evidence and a file relay do not establish real SMTP delivery.
set -euo pipefail
export CS_MAIL_DEMO_EXTERNAL=1
exec "$(dirname "$0")/m3b-demo.sh"
