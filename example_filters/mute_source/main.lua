if post.source:find "Peing" or
   post.source:find "ツイ廃あらーと" or
   post.source:find "今日のツイライフ" or
   post.source:find "ツイ廃ジャー" or
   post.source:find "contributter" or
   post.source:find "twttbot.net" or
   post.source:find "Githubiter" then
  return nil
else
  return post
end
