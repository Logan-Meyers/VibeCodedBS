import csv
import os
import sys

# ── Config ─────────────────────────────────────────────────────────────────────
PA_NUMBER   = 4
INPUT_FILE  = os.path.expanduser("~/Desktop/feedback.csv")
OUTPUT_FILE = os.path.expanduser(f"~/Desktop/pa{PA_NUMBER}_feedback.txt")

# ── Column widths (inner content, not counting border or padding) ───────────────
W_PTS   = 9   # at most 3-digit score; hardcoded
W_POSBL = 18  # 'pts possible' header fits; hardcoded
W_DESC  = 44  # minimum; expands to fit data
W_NOTES = 61  # minimum; expands to fit data

# pts column: main scores right-justified ending at PTS_MAIN_SLOT_END (exclusive)
PTS_MAIN_SLOT_END    = 4
# pts possible column: main scores right-justified ending at POSBL_MAIN_SLOT_END
POSBL_MAIN_SLOT_END  = 7
# pts possible column: sub scores left-justified from POSBL_SUB_SLOT_START
POSBL_SUB_SLOT_START = 9

# ── Derived constants (recomputed by compute_widths() after parsing) ────────────
INNER_TOTAL = 0  # placeholder


def compute_widths(sections):
    """Expand W_DESC / W_NOTES to fit content, then recompute INNER_TOTAL."""
    global W_DESC, W_NOTES, INNER_TOTAL

    for sec in sections:
        for _, _, desc, notes in [sec["main"]] + sec["subs"]:
            W_DESC  = max(W_DESC,  len(desc))
            W_NOTES = max(W_NOTES, len(notes))

    _sample     = f"| {'':^{W_PTS}} | {'':^{W_POSBL}} | {'':^{W_DESC}} | {'':^{W_NOTES}} |"
    INNER_TOTAL = len(_sample) - 4


# ── Table primitives ────────────────────────────────────────────────────────────
def hline_titled(col_titles):
    """Top border with column titles embedded: +-[ title ]-+-[ title ]-+..."""
    segs = []
    for title, width in col_titles:
        seg_width = width + 2
        tag   = f"-[ {title} ]-"
        pad   = seg_width - len(tag)
        left  = pad // 2
        right = pad - left
        segs.append("-" * left + tag + "-" * right)
    return "+" + "+".join(segs) + "+"

def hline():
    """Plain horizontal separator spanning all four columns."""
    return "+" + "+".join("-" * (w + 2) for w in (W_PTS, W_POSBL, W_DESC, W_NOTES)) + "+"

def full_hline():
    """Horizontal separator spanning the entire row width."""
    return "+" + "-" * (INNER_TOTAL + 2) + "+"

def row(pts_field, posbl_field, desc, notes):
    """Render a standard 4-column data row."""
    return (f"| {pts_field} "
            f"| {posbl_field} "
            f"| {desc:<{W_DESC}} "
            f"| {notes:<{W_NOTES}} |")

def full_row(text):
    """Render a row spanning all columns."""
    return f"| {text:<{INNER_TOTAL}} |"


# ── Score field formatters ──────────────────────────────────────────────────────
def _place(field_width, s, end_idx=None, start_idx=None):
    """Place string s into a blank field at a specific slot.
    end_idx:   right-justify s so its last char lands at end_idx - 1.
    start_idx: left-justify  s starting at start_idx.
    """
    field = [" "] * field_width
    if end_idx is not None:
        for i, ch in enumerate(s):
            field[end_idx - len(s) + i] = ch
    elif start_idx is not None:
        for i, ch in enumerate(s):
            field[start_idx + i] = ch
    return "".join(field)

def fmt_pts_main(pts):
    return _place(W_PTS, str(pts), end_idx=PTS_MAIN_SLOT_END)

def fmt_pts_sub(pts):
    return _place(W_PTS, str(pts), start_idx=PTS_MAIN_SLOT_END + 1)

def fmt_posbl_main(pts):
    return _place(W_POSBL, str(pts), end_idx=POSBL_MAIN_SLOT_END)

def fmt_posbl_sub(pts):
    return _place(W_POSBL, str(pts), start_idx=POSBL_SUB_SLOT_START)


# ── CSV parsing ─────────────────────────────────────────────────────────────────
def parse_csv(path):
    """Returns a list of section dicts: {main: (pts, posbl, desc, notes), subs: [...]}"""
    sections = []
    current  = None

    with open(path, newline="", encoding="utf-8") as f:
        for entry in csv.DictReader(f):
            is_sub = entry["subsection"].strip() == "1"
            rec    = (
                int(entry["points"].strip()),
                int(entry["points possible"].strip()),
                entry["description"].strip(),
                entry["notes"].strip(),
            )
            if not is_sub:
                current = {"main": rec, "subs": []}
                sections.append(current)
            else:
                if current is None:
                    raise ValueError("Sub-section found before any main section.")
                current["subs"].append(rec)

    return sections


# ── Rendering ───────────────────────────────────────────────────────────────────
def render(sections, closing_note):
    lines = []

    lines.append(hline_titled([
        ("pts",          W_PTS),
        ("pts possible", W_POSBL),
        ("Description",  W_DESC),
        ("Notes",        W_NOTES),
    ]))

    for i, sec in enumerate(sections):
        mpts, mposbl, mdesc, mnotes = sec["main"]

        if i > 0:
            lines.append(hline())

        lines.append(row(fmt_pts_main(mpts), fmt_posbl_main(mposbl), mdesc, mnotes))

        for spts, sposbl, sdesc, snotes in sec["subs"]:
            lines.append(row(fmt_pts_sub(spts), fmt_posbl_sub(sposbl), sdesc, snotes))

    total_pts   = sum(s["main"][0] for s in sections)
    total_posbl = sum(s["main"][1] for s in sections)
    lines.append(hline())
    lines.append(full_row(f"Total: {total_pts}/{total_posbl}"))

    lines.append(full_hline())
    lines.append(full_row(closing_note))
    lines.append(full_hline())

    return "\n".join(lines)


# ── Entry point ─────────────────────────────────────────────────────────────────
if __name__ == "__main__":
    if not os.path.exists(INPUT_FILE):
        print(f"Error: Input file not found: {INPUT_FILE}", file=sys.stderr)
        sys.exit(1)

    sections = parse_csv(INPUT_FILE)
    compute_widths(sections)

    closing_note = input("Enter closing note for student: ").strip()
    output       = render(sections, closing_note)

    with open(OUTPUT_FILE, "w", encoding="utf-8") as f:
        f.write(output)

    print(f"Written to {OUTPUT_FILE}")