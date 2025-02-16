# wikiplugin

A Neovim plugin for my personal wiki

# Installation

This plugin is dependent on [telescope.nvim](https://github.com/nvim-telescope/telescope.nvim) and has a small build script in Python, so you will need to install both of those.

I only use [vim-plug](https://github.com/junegunn/vim-plug), so that's the only plugin manager that I know how to use, but other plugin managers seem to be easily compatible too.

Add the following into init.vim:

```vim
Plug 'trampolinecall/wikiplugin', { 'do': './build.py' }
```

# Todo

- [ ] more consistent error handling with panics
- [ ] write documentation about config options
