local job_id = 0

local function setup(config)
    local function check_key_present(key)
        if config[key] == nil then
            error("wikiplugin setup config missing key '" .. key .. "'")
        end
    end
    check_key_present('home_path')
    check_key_present('note_id_timestamp_format')
    check_key_present('date_format')
    check_key_present('time_format')

    local plugin_root_path = vim.fn.fnamemodify(debug.getinfo(1).source:sub(2), ":p:h:h")
    local bin_path
    if (vim.fn.has('win32') ~= 0) or (vim.fn.has('win64') ~= 0) then
        bin_path = plugin_root_path .. '/target/release/wikiplugin.exe'
    else
        bin_path = plugin_root_path .. '/target/release/wikiplugin'
    end

    if not vim.fn.filereadable(bin_path) then
        error("failed to find wikiplugin binary at path '" .. bin_path .. "'")
    end

    if job_id ~= 0 then
        vim.fn.jobstop(job_id)
        job_id = 0
    end

    job_id = vim.fn.jobstart({ bin_path, config.home_path, config.note_id_timestamp_format, config.date_format, config.time_format }, { rpc = true })

    if job_id == 0 then
        error("failed to connect to the rpc endpoint with path '" .. bin_path "'")
    elseif job_id == -1 then
        error("binary '" .. bin_path .. "' is not executable")
    end
end

local function check_job_running()
    if job_id == 0 then
        error("wikiplugin job is not running")
    end
end

-- TODO: automate these functions?
local function new_note(directory, focus)
    check_job_running()
    vim.fn.rpcnotify(job_id, "new_note", directory, focus)
end
local function open_index()
    check_job_running()
    vim.fn.rpcnotify(job_id, "open_index")
end
local function new_note_and_insert_link()
    check_job_running()
    vim.fn.rpcnotify(job_id, "new_note_and_insert_link")
end

return {
    setup = setup,

    new_note = new_note,
    open_index = open_index,
    new_note_and_insert_link = new_note_and_insert_link,
}
