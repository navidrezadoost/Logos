import React from "react";
import type { SVGProps } from "react";

export interface PositionProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const Position = React.forwardRef<SVGSVGElement, PositionProps>(
  ({ size, className, style, ...props }, ref) => {
    const dimensions = size ? { width: size, height: size } : {
      width: props.width || 24,
      height: props.height || 24
    };

    return (
      <svg
        ref={ref}
        viewBox="0 0 640 640"
        xmlns="http://www.w3.org/2000/svg"
        width={dimensions.width}
        height={dimensions.height}
        fill={props.fill || "currentColor"}
        className={className}
        style={style}
        {...props}
      >
        <path fill="currentColor" d="M544 96L96 96L96 544L544 544L544 96zM480 160L480 224L160 224L160 160L480 160zM160 288L224 288L224 480L160 480L160 288zM480 288L480 480L288 480L288 288L480 288z"/>
      </svg>
    );
  }
);

Position.displayName = "Position";

export default Position;
