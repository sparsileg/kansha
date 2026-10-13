-- Starts the appendices at the first chapter heading marked {.appendix}:
--   # Keyboard Shortcuts {.appendix}
-- From there on, chapters are lettered (Appendix A, B, ...) instead of
-- numbered. Every chapter after the first appendix is an appendix too.
local started = false

function Header(h)
  if h.level == 1 and h.classes:includes('appendix') and not started then
    started = true
    return {
      pandoc.RawBlock('typst', '#counter(heading).update(0)\n'
        .. '#show heading.where(level: 1, outlined: true): set heading(numbering: "A", supplement: [Appendix])'),
      h,
    }
  end
end
