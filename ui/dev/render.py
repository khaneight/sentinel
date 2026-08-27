import json, sys

d = json.load(sys.stdin)
if not d["count"]:
    print("nothing pending")
    raise SystemExit

for i, a in enumerate(d["annotations"], 1):
    print()
    print("[%d] %s" % (i, a["comment"]))
    print("    element  %s  (%s)" % (a["element"], a["fullPath"]))
    if a.get("nearbyText"):
        print("    near     %s" % a["nearbyText"])
    print("    page     %s" % a["url"])
