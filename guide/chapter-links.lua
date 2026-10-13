-- Points links to another chapter's file (registers.md) at that
-- chapter's heading. --file-scope gives each heading an id prefixed with
-- its file path (chapters__registers.md__splits).
function Pandoc(doc)
  local chapters = {}
  doc.blocks:walk({
    Header = function(h)
      local file = h.identifier:match("([%w%-]+%.md)__")
      if h.level == 1 and file and not chapters[file] then
        chapters[file] = h.identifier
      end
    end
  })
  return doc:walk({
    Link = function(link)
      local id = chapters[link.target]
      if id then
        link.target = "#" .. id
        return link
      end
    end
  })
end
