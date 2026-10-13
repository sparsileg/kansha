-- Turns fenced divs into the callout boxes defined in guide.typ:
--   ::: tip       Kansha Tip
--   ::: why       Why this matters
--   ::: note      Note
--   ::: shot      Screenshot placeholder, saying what the picture will show.
--                 Give each an id ({.shot id="new-book"}) naming the
--                 image that replaces it: images/<id>.png (a capture
--                 of the app, saved by hand). Without the file, the box stays.
-- A title="..." attribute replaces the default heading.
local kinds = { tip = true, why = true, note = true, shot = true }

local function exists(path)
  local file = io.open(path, 'r')
  if file then file:close() end
  return file ~= nil
end

function Div(div)
  -- --file-scope prefixes ids with the chapter file (chapters__x.md__id)
  local id = div.identifier:match('([^_]+)$') or ''
  for _, class in ipairs(div.classes) do
    if class == 'shot' and exists('images/' .. id .. '.png') then
      local blocks = pandoc.List({ pandoc.RawBlock('typst', '#screenshot("/images/' .. id .. '.png")[') })
      blocks:extend(div.content)
      blocks:insert(pandoc.RawBlock('typst', ']'))
      return blocks
    end
    if kinds[class] then
      local title = div.attributes.title
      local args = '"' .. class .. '"'
      if title then
        args = args .. ', title: "' .. title:gsub('\\', '\\\\'):gsub('"', '\\"') .. '"'
      end
      local blocks = pandoc.List({ pandoc.RawBlock('typst', '#callout(' .. args .. ')[') })
      blocks:extend(div.content)
      blocks:insert(pandoc.RawBlock('typst', ']'))
      return blocks
    end
  end
end
