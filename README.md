# csview

Tauri mac os app to view CSV files.

# Features

Can be launched from the command line or as an app associated with CSV (should associate .csv with it when installed / opened optionally).

The apps shows a datagrid and shows columns and rows like a regular spreadsheet app. It should look really nice, with a clean look and feel, and we assume the first row is a header, especially if it looks like a header, but this should be toggled if needed.

The app should not load the whole csv into memory unless it is very small. Generally, it should take a small sample of the first N lines and use that to populate the headers and some rows. If the file is large, there should be a "load the rest or load stats" or some other intelligent processing buttons and they should carefully do things without blocking the thread or locking up or anything like that.

I want basic resizing, sorting and switching direction of sort, multi-column sorting, a text typeahead search, basic stats about the CSV file, etc.

I want to be able to resize cells and rows too.

# Development

Always write tests, they must be comprehensive and must cover all functionality.
Always run tests when changing things
When you are done with work and are ready to show it off, launch the app, take a screenshot of the app, and deeply consider if the app looks like how you expect. Walk through the app yourself programmatically to make sure it all works and loads and looks good. Continue to fix it, add tests, and validate until it is really polished.

