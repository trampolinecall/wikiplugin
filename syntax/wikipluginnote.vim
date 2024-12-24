if exists('b:current_syntax')
    finish
endif

runtime! syntax/markdown.vim syntax/markdown_*.vim syntax/markdown/*.vim
