local status_code, message = http_get("https://vorona.gg/api/book/BloodOath")

if status_code == 200
then
    return "Ok"
else
    error(message, 0)
end
