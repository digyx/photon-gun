local status_code, message = photon.http.get("https://vorona.gg/api/book/BloodOath")

if status_code == 200
then
    return "Rlua works!!"
else
    error(message, 0)
end
