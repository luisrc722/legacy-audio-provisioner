# Playbook: Reprovisión y Auditoría Manual de Archivos en USB

## 1. Preparar el entorno y compilar el binario

```sh
cd /home/dev/Documents/Projects/_separado/proyectos_externos/legacy-audio-provisioner
cargo build --release
```
**Evidencia esperada:**
- Compilación exitosa, binario en `target/release/lap-bin-provision`.

---

## 2. Identificar y montar la USB

- Conecta la USB y verifica el punto de montaje:
```sh
lsblk
mount | grep media
```
**Evidencia esperada:**
- USB montada en `/media/dev/6A08-0A02` (ajusta según tu sistema).

---

## 3. Auditar la cuarentena de la USB

- Listar archivos pendientes en cuarentena:
```sh
ls -l /media/dev/6A08-0A02/.legacy_quarantine/sync_/
ls -l /media/dev/6A08-0A02/.legacy_quarantine/musica_sucia/
```
**Evidencia esperada:**
- Listado de archivos pendientes en ambas carpetas.

---

## 4. Crear staging local para reprovisión

```sh
mkdir -p /home/dev/prueba_usb_restante
cp /media/dev/6A08-0A02/.legacy_quarantine/sync_/* /home/dev/prueba_usb_restante/
cp /media/dev/6A08-0A02/.legacy_quarantine/musica_sucia/* /home/dev/prueba_usb_restante/
ls /home/dev/prueba_usb_restante | wc -l
```
**Evidencia esperada:**
- Todos los archivos pendientes copiados a `/home/dev/prueba_usb_restante`.
- El conteo debe coincidir con la suma de archivos en cuarentena.

---

## 5. Ejecutar reprovisión sobre los archivos pendientes

```sh
LAP_SAFE_EJECT=0 ./target/debug/lap-bin-provision provision --sync --usb /media/dev/6A08-0A02 --source /home/dev/prueba_usb_restante --lang es | tee /tmp/lap_pending_restante.log
```
**Evidencia esperada:**
- Proceso inicia, muestra logs en consola y guarda en `/tmp/lap_pending_restante.log`.
- Se crea/actualiza el lock: `/media/dev/6A08-0A02/.lap_provisioning.lock`.

---

## 6. Monitorear el proceso

- Verifica si el proceso sigue activo:
```sh
cat /media/dev/6A08-0A02/.lap_provisioning.lock
ps aux | grep <PID>
```
- Revisa el log para ver el avance:
```sh
tail -n 40 /tmp/lap_pending_restante.log
```
**Evidencia esperada:**
- El PID existe mientras el proceso está corriendo.
- El log muestra fases: “diff hash USB”, “provisioning”, “COMMAND_END” al finalizar.

---

## 7. Validar resultados tras finalizar

- Espera a que el proceso termine (`COMMAND_END` en el log).
- Revisa que los archivos hayan sido procesados:
```sh
ls -l /media/dev/6A08-0A02/.legacy_quarantine/sync_/
ls -l /media/dev/6A08-0A02/.legacy_quarantine/musica_sucia/
ls -l /media/dev/6A08-0A02/musica/
```
**Evidencia esperada:**
- Las carpetas de cuarentena deben vaciarse o reducirse.
- Los archivos limpios aparecen en `/musica/`.

---

## 8. Auditoría final y desmontaje seguro

- Verifica que no haya lock ni procesos activos:
```sh
ls /media/dev/6A08-0A02/.lap_provisioning.lock
# Si existe, revisa y elimina si es seguro
```
- Desmonta la USB si es necesario:
```sh
umount /media/dev/6A08-0A02
```
**Evidencia esperada:**
- USB desmontada correctamente.

---

## 9. Notas y verificaciones adicionales

- Si algún archivo sigue en cuarentena, revisa logs para errores específicos.
- El proceso de normalización y limpieza de metadatos/portada se realiza automáticamente (ver código en `normalizer.rs`).
- El binario respeta la política de mantener la USB montada por defecto (`LAP_SAFE_EJECT=0`).

---

## Resumen de comandos clave

```sh
cargo build --release
lsblk
mount | grep media
ls -l /media/dev/6A08-0A02/.legacy_quarantine/sync_/
ls -l /media/dev/6A08-0A02/.legacy_quarantine/musica_sucia/
mkdir -p /home/dev/prueba_usb_restante
cp ... # (ver arriba)
LAP_SAFE_EJECT=0 ./target/debug/lap-bin-provision provision --sync --usb /media/dev/6A08-0A02 --source /home/dev/prueba_usb_restante --lang es | tee /tmp/lap_pending_restante.log
cat /media/dev/6A08-0A02/.lap_provisioning.lock
ps aux | grep <PID>
tail -n 40 /tmp/lap_pending_restante.log
ls -l /media/dev/6A08-0A02/musica/
umount /media/dev/6A08-0A02
```

---

**Con esto puedes replicar todo el proceso de reprovisión, auditoría y validación de archivos en la USB, asegurando limpieza, trazabilidad y evidencia en cada paso.**
