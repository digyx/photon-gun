local status_code, msg = photon.http.get("WIREMOCK_URI/healthcheck")

if status_code == 200
    then
        return "Alles gut"
    else
        error("This failed", 0)
    end
