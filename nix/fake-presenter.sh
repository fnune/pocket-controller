#!/bin/sh
printf '%s\n' "$2" >>/tmp/presenter.log
exec sleep 100000
