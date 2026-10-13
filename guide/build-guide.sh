#!/usr/bin/env bash
# Builds the Kansha User's Guide PDF from the Markdown chapters (UI-048).
# Needs pandoc (3.x) and typst on the PATH.
# Usage: ./build-guide.sh [output.pdf]
#   (default: ../src-tauri/help/kansha-guide.pdf, which the app bundles)
set -euo pipefail
cd "$(dirname "$0")"

# Chapters in reading order
chapters=(
    chapters/introduction.md
    chapters/getting-started.md
    chapters/coming-from-quicken.md
    chapters/first-month.md
    chapters/accounts.md
    chapters/categories.md
    chapters/register.md
    chapters/scheduled.md
    chapters/reconciling.md
    chapters/investments.md
    chapters/reports.md
    chapters/insights.md
    chapters/command-line.md
    chapters/backups.md
    chapters/settings.md
    chapters/recipes.md
    chapters/troubleshooting.md
    chapters/appendix-reports.md
    chapters/appendix-kansha-add.md
    chapters/appendix-shortcuts.md
    chapters/appendix-glossary.md
)

version=$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' ../package.json | head -n1)
output=${1:-../src-tauri/help/kansha-guide.pdf}

mkdir -p build "$(dirname "$output")"
# The chapters often start a list on the line after a paragraph, with no
# blank line between; --file-scope turns links between chapter files into
# links inside the PDF
pandoc -f markdown+lists_without_preceding_blankline --file-scope --lua-filter chapter-links.lua --lua-filter callouts.lua --lua-filter appendices.lua --template guide.typ -V version="$version" \
    -t typst -o build/kansha-guide.typ "${chapters[@]}"
typst compile --root . --font-path fonts --ignore-system-fonts \
    build/kansha-guide.typ "$output"
echo "Built $output (version $version)"
