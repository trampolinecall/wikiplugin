if exists('b:current_syntax')
    finish
endif

" TODO: do not have a custom syntax file or filetype and just allow the filetype to be markdown with autocommands on top

runtime! syntax/markdown.vim syntax/markdown_*.vim syntax/markdown/*.vim
unlet b:current_syntax

syn iskeyword 48-57,a-z,A-Z,_
syn keyword wikipluginAutogenerateBounds wikiplugin_autogenerate wikiplugin_autogenerate_end
hi link wikipluginAutogenerateBounds Comment

let b:current_syntax = 'wikipluginnote'
