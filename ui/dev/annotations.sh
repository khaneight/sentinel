#!/usr/bin/env bash
# Read (and clear) annotations left on the showcase page.
#
#   ./annotations.sh          list what is pending, readable
#   ./annotations.sh --json   the same, raw
#   ./annotations.sh --clear  resolve everything pending
#
# Talks to the agentation-mcp HTTP server on :4747. The MCP tools do the same
# thing from inside a coding agent; this exists so the loop works without one,
# and without restarting a session to pick up a newly registered MCP server.
set -euo pipefail
HOST=${AGENTATION_HOST:-http://localhost:4747}
HERE=$(cd "$(dirname "$0")" && pwd)

case "${1:-list}" in
  --json)
    curl -s "$HOST/pending"
    ;;
  --clear)
    n=0
    for id in $(curl -s "$HOST/pending" | python3 -c 'import json,sys; [print(a["id"]) for a in json.load(sys.stdin)["annotations"]]'); do
      curl -s -o /dev/null -X DELETE "$HOST/annotations/$id"
      n=$((n + 1))
    done
    echo "cleared $n annotation(s)"
    ;;
  *)
    curl -s "$HOST/pending" | python3 "$HERE/render.py"
    ;;
esac
