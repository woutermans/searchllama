Disclaimer:
 Just a random project. Don't expect much. I'm a terrible programmer.
 needs serious rework if you want to use it in production.

everything should be able to compile with cargo + trunk + python (with duckduckgo_search installed). 
Ollama is used for the LLM calls. nomic-embed-text for embedding. llama3.1:latest for other stuff.

What it does (sometimes):
  asks the llm for search queries related to the question and then forwards them to duckduckgo.
  Scrapes the sites duckduckgo gives and then uses nomic-embed-text to rank them for relevance.
  tries to guess wether to give you an LLM answer (example: "how to push to github") or only give you links (example: "github")
  uses cached and a sqlite db to speed up subsequent queries.
