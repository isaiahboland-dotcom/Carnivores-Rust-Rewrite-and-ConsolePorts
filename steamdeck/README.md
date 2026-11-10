# Steamdeck installation script for the gui launcher

to run the installation script you can run this one liner here:

```
curl -fsSL https://raw.githubusercontent.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/main/steamdeck/installscript.sh | bash

#to update:

 curl -fsSL https://raw.githubusercontent.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/main/steamdeck/installscript.sh | bash -s -- --update
 ```

# Notice
I recommend that you setup your instances in desktop mode first, to ensure you have your file paths set correctly. Trying to set the folder path currently doesnt work in big picture mode because it tries bringing up a pop up window and can sometimes behave eradically. Outside of setting up the folder locations works in big picture just fine.
Also you will definitely want to setup a custom controller scheme for the time being, we are working on smoother controller integration on the steamdeck
