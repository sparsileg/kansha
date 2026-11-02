-- Builds the guide's index from index-terms.txt (see its header for the
-- format). Each section's first mention of a term gets an invisible
-- mark (#metadata("Term")<kidx>) at the start of its paragraph; a term
-- in a heading marks the paragraph after it. At the end of the book it
-- adds the Index chapter, which guide.typ's make-index draws from the
-- marks, with page numbers and the "see" entries.

local terms = {} -- { name, alts = { { pattern, heading_only } } }
local sees = {} -- { from, to }

local function trim(s)
  return (s:gsub('^%s+', ''):gsub('%s+$', ''))
end

-- A Lua pattern matching `word` as a whole word, any case, plural s/es.
local function pattern(word)
  local esc = word:lower():gsub('[%(%)%.%%%+%-%*%?%[%]%^%$]', '%%%0')
  local first, last = word:sub(1, 1), word:sub(-1)
  local p = esc
  if first:match('%w') then p = '%f[%w]' .. p end
  if last:match('%w') then p = p .. 'e?s?%f[^%w]' end
  return p
end

local function load()
  local file = io.open('index-terms.txt', 'r')
  if not file then error('index.lua: index-terms.txt not found') end
  for line in file:lines() do
    line = trim(line)
    if line ~= '' and not line:match('^#') then
      local from, to = line:match('^(.-)%s*=>%s*(.+)$')
      if from then
        table.insert(sees, { from = from, to = to })
      else
        local name, words = line:match('^(.-)%s*=%s*(.+)$')
        if not name then name, words = line, line end
        local alts = {}
        for w in (words .. ';'):gmatch('([^;]*);') do
          w = trim(w)
          if w ~= '' then
            local heading_only = w:sub(1, 1) == '#'
            if heading_only then w = trim(w:sub(2)) end
            table.insert(alts, { p = pattern(w), heading_only = heading_only })
          end
        end
        table.insert(terms, { name = name, alts = alts })
      end
    end
  end
  file:close()
end

local function plain(el)
  return ' ' .. pandoc.utils.stringify(el):lower():gsub('\226\128\153', "'") .. ' '
end

local function quote(s)
  return '"' .. s:gsub('\\', '\\\\'):gsub('"', '\\"') .. '"'
end

-- The terms first mentioned in `text` in this section.
local seen = {}
local function found(text, heading)
  local out = {}
  for _, t in ipairs(terms) do
    if not seen[t.name] then
      for _, a in ipairs(t.alts) do
        if (heading or not a.heading_only) and text:find(a.p) then
          seen[t.name] = true
          table.insert(out, t.name)
          break
        end
      end
    end
  end
  return out
end

local pending = {}
local function marked(el)
  local names = found(plain(el), false)
  for _, n in ipairs(pending) do table.insert(names, 1, n) end
  pending = {}
  if #names == 0 then return nil end
  local raw = {}
  for _, n in ipairs(names) do table.insert(raw, '#metadata(' .. quote(n) .. ')<kidx>') end
  el.content:insert(1, pandoc.RawInline('typst', table.concat(raw)))
  return el
end

function Pandoc(doc)
  load()
  doc = doc:walk({
    traverse = 'topdown',
    Header = function(h)
      if h.level <= 2 then seen = {} end
      for _, n in ipairs(found(plain(h), true)) do table.insert(pending, n) end
      return h, false
    end,
    CodeBlock = function(c) return c, false end,
    Para = function(p) return marked(p), false end,
    Plain = function(p) return marked(p), false end,
  })
  local see = {}
  for _, s in ipairs(sees) do
    table.insert(see, '(' .. quote(s.from) .. ', ' .. quote(s.to) .. ')')
  end
  doc.blocks:insert(pandoc.RawBlock('typst',
    '#heading(level: 1, numbering: none)[Index]\n#make-index(see: (' .. table.concat(see, ', ') .. ',))'))
  return doc
end
