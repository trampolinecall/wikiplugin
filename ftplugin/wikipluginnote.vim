if exists("b:did_ftplugin")
  finish
endif

runtime! ftplugin/markdown.vim ftplugin/markdown_*.vim ftplugin/markdown/*.vim

augroup wikipluginnote_ftplugin
    " autocmd BufNewFile,BufRead,BufWritePre <buffer> lua require('wikiplugin').regenerate_autogenerated_sections() TODO
augroup END
