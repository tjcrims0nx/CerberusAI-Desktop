#!/bin/bash
sed -i 's|"/message"|"/skills-message"|g' /root/skills-server/src/http.ts
sed -i 's|"/sse"|"/skills-sse"|g' /root/skills-server/src/http.ts
sed -i 's|app.post("/message"|app.post("/skills-message"|g' /root/skills-server/src/http.ts
sed -i 's|app.get("/sse"|app.get("/skills-sse"|g' /root/skills-server/src/http.ts
cd /root/skills-server
npm run build
systemctl restart cerberus-skills
