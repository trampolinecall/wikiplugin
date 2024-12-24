# wikiplugin

A Neovim plugin for my personal wiki

# Installation

I only use [vim-plug](https://github.com/junegunn/vim-plug), so that's the only plugin manager that I know how to use. But, like with other plugins, other plugin managers seem to be compatible too.

Add the following into init.vim:

```vim
Plug 'trampolinecall/wikiplugin', { 'do': 'cargo build --release' }
```

# Todo

- [ ] more consistent error handling with panics
- [ ] write documentation about config options
- [ ] find some way to automatically make sure that messages sent are the same on the lua and rust side
- [ ] add custom filetype
- [ ] properly handle PathBuf not being utf8
