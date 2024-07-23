" TODO: add support for restarting
" finish if the job is already started
if exists('s:jobid') && s:jobid > 0
    finish
endif

" find the binary path
if has('win32') || has('win64')
    let s:bin = expand("<sfile>:p:h:h") . '/target/release/wikiplugin.exe'
else
    let s:bin = expand("<sfile>:p:h:h") . '/target/release/wikiplugin'
endif

" start the job
if !filereadable(s:bin)
    echoerr printf('failed to find wikiplugin binary at path %s', s:bin)
    finish
endif

let s:jobid = jobstart([s:bin], { 'rpc': v:true })

if s:jobid == 0
    echoerr printf('failed to connect to the rpc endpoint: [%s]', s:bin)
    finish
elseif s:jobid == -1
    echoerr printf('binary [%s] is not executable', s:bin)
    finish
endif

" bind commands
