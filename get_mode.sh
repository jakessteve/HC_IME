#!/bin/bash
# Get the mode from Fcitx5 via DBus to absolutely confirm what Fcitx5 thinks the config is
qdbus org.fcitx.Fcitx5 /controller org.fcitx.Fcitx.Controller1.Config "hcime"
