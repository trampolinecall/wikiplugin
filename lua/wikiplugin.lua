local job_id = 0
local config = nil

local function setup(config_local)
    local function check_key_present(key)
        if config_local[key] == nil then
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
        config = nil
    end

    job_id = vim.fn.jobstart({ bin_path, config_local.home_path, config_local.note_id_timestamp_format, config_local.date_format, config_local.time_format }, { rpc = true })
    config = config_local

    if job_id == 0 then
        error("failed to connect to the rpc endpoint with path '" .. bin_path "'")
        config = nil
    elseif job_id == -1 then
        error("binary '" .. bin_path .. "' is not executable")
        config = nil
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
local function insert_link_at_cursor(link_to_id, link_text)
    check_job_running()
    vim.fn.rpcnotify(job_id, "insert_link_at_cursor", link_to_id, link_text)
end
local function insert_link_at_cursor_or_create(link_to_id, link_text)
    check_job_running()
    vim.fn.rpcnotify(job_id, "insert_link_at_cursor_or_create", link_to_id, link_text)
end
local function delete_note()
    check_job_running()
    vim.fn.rpcnotify(job_id, "delete_note")
end

local function insert_link_attach_mappings(prompt_bufnr, map)
    local actions = require "telescope.actions"
    local action_state = require "telescope.actions.state"

    actions.select_default:replace(function()
        actions.close(prompt_bufnr)
        local selection = action_state.get_selected_entry()
        local note_id
        if selection then
            note_id = selection.note_id
        else
            note_id = nil
        end

        insert_link_at_cursor_or_create(note_id, nil)
    end)
    return true
end
local function search_by_title(attach_mappings, opts)
    local pickers = require "telescope.pickers"
    local finders = require "telescope.finders"
    local conf = require("telescope.config").values

    opts = opts or {}
    pickers.new(opts, {
        prompt_title = "search notes by title",

        finder = finders.new_oneshot_job(
            {"ag", "^title:", config.home_path}, -- TODO: this does not work perfectly because it can match any 'title: ' that appears outside of the frontmatter but whatever
            {
                entry_maker = function(entry)
                    local parts = vim.split(entry, ':')

                    local note_title = parts[4]:match("^%s*(.-)%s*$") -- TODO: put this into a trim whitespace function

                    local filepath = parts[1]
                    -- TODO: properly support notes in other directories (because these are not reflected in the note id)
                    local note_id = vim.split(vim.fs.basename(filepath), '.', { plain = true })[1]

                    return {
                        value = entry,
                        display = note_title,
                        ordinal = note_title,
                        note_id = note_id,
                    }
                end,
            }
        ),

        sorter = conf.generic_sorter(opts),
        previewer = conf.grep_previewer(opts),

        attach_mappings = attach_mappings,
    }):find()
end
local function search_by_content(attach_mappings)
    local pickers = require "telescope.pickers"
    local finders = require "telescope.finders"
    local conf = require("telescope.config").values

    opts = opts or {}
    pickers.new(opts, {
        prompt_title = "search notes by content",

        finder = finders.new_oneshot_job(
            {"ag", "^", config.home_path}, -- this is probably not the best way to do this
            {
                entry_maker = function(entry)
                    local parts = vim.split(entry, ':')

                    local filepath = parts[1]
                    local note_id = vim.split(vim.fs.basename(filepath), '.', { plain = true })[1]

                    return {
                        value = entry,
                        display = entry,
                        ordinal = entry,
                        note_id = note_id,
                    }
                end,
            }
        ),

        sorter = conf.generic_sorter(opts),
        previewer = conf.grep_previewer(opts),

        attach_mappings = attach_mappings,
    }):find()
end
local function insert_link_by_title()
    search_by_title(insert_link_attach_mappings)
end
local function insert_link_by_content()
    search_by_content(insert_link_attach_mappings)
end

return {
    setup = setup,

    new_note = new_note,
    open_index = open_index,
    new_note_and_insert_link = new_note_and_insert_link,
    search_by_title = search_by_title,
    search_by_content = search_by_content,
    insert_link_by_title = insert_link_by_title,
    insert_link_by_content = insert_link_by_content,
    delete_note = delete_note,
}
