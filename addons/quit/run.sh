#!/bin/zsh
# Lists running apps (from lsappinfo, which needs no permissions).
# Choosing one asks it to quit politely, so it can still prompt to save.
q=${1:l}
lsappinfo list | awk -v q="$q" '
function esc(s) { gsub(/\\/, "\\\\", s); gsub(/"/, "\\\"", s); return s }
function flush() {
  if (type == "Foreground" && name != "" && bid != "" && name != "Launcher" && (q == "" || index(tolower(name), q))) {
    printf "%s{\"uid\":\"%s\",\"title\":\"%s\",\"subtitle\":\"Quit\",\"icon\":{\"kind\":\"file\",\"value\":\"%s\"},", sep, esc(bid), esc(name), esc(path)
    printf "\"action\":{\"type\":\"run\",\"argv\":[\"/usr/bin/osascript\",\"-e\",\"tell application id \\\"%s\\\" to quit\"]}}", esc(bid)
    sep = ","
  }
  name = ""; path = ""; type = ""; bid = ""
}
BEGIN { printf "{\"items\":[" }
/^ *[0-9]+\) "/ { flush(); match($0, /"[^"]*"/); name = substr($0, RSTART + 1, RLENGTH - 2) }
/bundleID="/   { match($0, /bundleID="[^"]*"/); bid = substr($0, RSTART + 10, RLENGTH - 11) }
/bundle path="/ { match($0, /bundle path="[^"]*"/); path = substr($0, RSTART + 13, RLENGTH - 14) }
/ type="/      { match($0, / type="[^"]*"/); type = substr($0, RSTART + 7, RLENGTH - 8) }
END { flush(); print "]}" }
'
