Poliy installation

```
checkmodule -M -m -o trueid.mod trueid.te
semodule_package -o trueid.pp -m trueid.mod
sudo semodule -i trueid.pp
```