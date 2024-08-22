# Disclaimer
This is a personal project, not intended for production use. The codebase is in need of significant improvement.

## Requirements
To compile and run this project, you'll need:
* Cargo
* Trunk
* Python with `duckduckgo_search` installed
* Ollama installation
* nomic-embed-text for text embedding
* llama3.1:latest for other tasks
* A system capable of running `playwright`

## Project Description
This project uses a Large Language Model (LLM) to generate search queries and scrape results from DuckDuckGo. It then ranks the results using nomic-embed-text and decides whether to provide an LLM answer or a list of links.

### Key Features
* Uses cached and SQLite database for faster subsequent queries
* Integrates with DuckDuckGo to fetch search results
* Employs Nomic-embed-text for text ranking and embedding
* Decides between providing an LLM answer or a list of links based on the query
