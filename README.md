# progressrm

View the progress of an rm command. Caveats:

 - this is still a PoC
 - the command must have lots of arguments, and probably be recursive, otherwise the algorithm and estimation won't work
 - if you need this, you probably are deleting on a hard disk and not an SSD

# Example usage
```
$ progressrm
[1452864] rm in /home/anisse/backups
        29.0% (402 / 1384 args) 4.4 args/h remaining 9 days 8:12:16
```
