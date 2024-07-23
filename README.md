# wikiplugin

A Neovim plugin for my personal wiki

# Installation

I only use [vim-plug](https://github.com/junegunn/vim-plug), so that's the only plugin manager that I know how to use, but other plugin managers seem to be compatible too.

Add the following into init.vim:

```vim
Plug 'trampolinecall/wikiplugin', { 'do': 'cargo build --release' }
```

# Todo

- [ ] more consistent error handling with panics
