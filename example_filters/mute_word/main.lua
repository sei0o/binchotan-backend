if post.text:find "大学" or post.text:find "ツイッター" then
  return nil
else
  return post
end

