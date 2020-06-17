# PSO2

Urgent quest discord bot

outer scheduler
- runs every day
- fetches latest urgent quest (needs to check the last two)
  - new article comes out, but we're still on the previous article schedule
- parses into utc time and event name (and url it was pulled from)
- kill previous scheduler
- create second scheduler with all these events (say notify 30 minutes earlier)

second scheduler (before it gets killed)
- notify event will happen at this time, what event, and the url to learn more
- https://pso2.com/news/urgent-quests/uqmay2020part3
